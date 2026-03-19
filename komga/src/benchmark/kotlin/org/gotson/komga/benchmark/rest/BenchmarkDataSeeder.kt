package org.gotson.komga.benchmark.rest

import org.gotson.komga.domain.model.Book
import org.gotson.komga.domain.model.BookPage
import org.gotson.komga.domain.model.BookMetadata
import org.gotson.komga.domain.model.Dimension
import org.gotson.komga.domain.model.KomgaUser
import org.gotson.komga.domain.model.Library
import org.gotson.komga.domain.model.Media
import org.gotson.komga.domain.model.Series
import org.gotson.komga.domain.model.UserRoles
import org.gotson.komga.domain.persistence.BookMetadataRepository
import org.gotson.komga.domain.persistence.BookRepository
import org.gotson.komga.domain.persistence.KomgaUserRepository
import org.gotson.komga.domain.persistence.LibraryRepository
import org.gotson.komga.domain.persistence.MediaRepository
import org.gotson.komga.domain.persistence.SeriesRepository
import org.gotson.komga.domain.service.SeriesLifecycle
import org.springframework.stereotype.Component
import java.awt.image.BufferedImage
import java.io.ByteArrayOutputStream
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.attribute.FileTime
import java.time.Instant
import java.time.LocalDate
import java.time.LocalDateTime
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import javax.imageio.ImageIO

@Component
class BenchmarkDataSeeder(
  private val userRepository: KomgaUserRepository,
  private val libraryRepository: LibraryRepository,
  private val seriesRepository: SeriesRepository,
  private val bookRepository: BookRepository,
  private val seriesLifecycle: SeriesLifecycle,
  private val mediaRepository: MediaRepository,
  private val bookMetadataRepository: BookMetadataRepository,
) {
  private val benchmarkRoot = Path.of("config-dir", "benchmark-fixtures")
  private val benchmarkInstant = Instant.parse("2024-01-01T00:00:00Z")
  private val benchmarkDateTime = LocalDateTime.of(2024, 1, 1, 0, 0)
  private val releaseBase = LocalDate.of(2024, 1, 1)
  private val mediaType = "application/zip"
  private val pageCount = 24
  private val fullRoles = UserRoles.entries.toSet()
  private val libraryCount = 5
  private val seriesPerLibrary = 100
  private val regularBookCount = 4
  private val anchorBookCount = 500
  private val scanLibraryName = "Benchmark Scan Library"
  private val scanSeriesName = "Benchmark Scan Series"
  private val scanBookName = "Benchmark Scan Book 0001"
  private val importLibraryName = "Benchmark Import Library"
  private val importSeriesName = "Benchmark Import Series"
  private val importSourceName = "benchmark-source"

  fun ensureSeedData() {
    ensureAdminUser()

    if (seriesRepository.count() > 0L || bookRepository.count() > 0L) {
      repairMediaDeliveryFixture()
      return
    }

    var releaseOffset = 0L
    repeat(libraryCount) { libraryIndex ->
      val library = ensureLibrary(libraryIndex)
      repeat(seriesPerLibrary) { seriesIndex ->
        val series = ensureSeries(library, libraryIndex, seriesIndex)
        val bookCount = if (libraryIndex == 0 && seriesIndex == 0) anchorBookCount else regularBookCount
        releaseOffset = ensureBooks(series, library, libraryIndex, seriesIndex, bookCount, releaseOffset)
      }
    }
  }

  fun ensureScanBenchmarkData(): Library {
    ensureAdminUser()

    val root = benchmarkRoot.resolve("scan-root")
    val seriesRoot = root.resolve("series-1")
    val bookPath = seriesRoot.resolve("book-0001.cbz")
    Files.createDirectories(seriesRoot)
    val pages = createBenchmarkBookArchive(bookPath)
    setFixedLastModified(root)
    setFixedLastModified(seriesRoot)
    setFixedLastModified(bookPath)

    libraryRepository.findAll().firstOrNull { it.name == scanLibraryName }?.let { return it }

    val library =
      Library(
        name = scanLibraryName,
        root = root.toUri().toURL(),
        createdDate = benchmarkDateTime,
        lastModifiedDate = benchmarkDateTime,
      )
    libraryRepository.insert(library)

    val series =
      Series(
        name = scanSeriesName,
        url = seriesRoot.toUri().toURL(),
        fileLastModified = benchmarkDateTime,
        libraryId = library.id,
        createdDate = benchmarkDateTime,
        lastModifiedDate = benchmarkDateTime,
      )
    seriesRepository.insert(series)
    val insertedSeries = seriesRepository.findAllByLibraryId(library.id).first { it.name == scanSeriesName }

    val book =
      Book(
        name = scanBookName,
        url = bookPath.toUri().toURL(),
        fileLastModified = benchmarkDateTime,
        fileSize = Files.size(bookPath),
        number = 1,
        libraryId = library.id,
        seriesId = insertedSeries.id,
        createdDate = benchmarkDateTime,
        lastModifiedDate = benchmarkDateTime,
      )
    bookRepository.insert(book)
    val persistedBook = bookRepository.findAllBySeriesId(insertedSeries.id).first { it.name == scanBookName }

    mediaRepository.insert(
      Media(
        status = Media.Status.READY,
        mediaType = mediaType,
        pageCount = pageCount,
        pages = pages,
        bookId = persistedBook.id,
        createdDate = benchmarkDateTime,
        lastModifiedDate = benchmarkDateTime,
      ),
    )
    bookMetadataRepository.insert(
      BookMetadata(
        title = scanBookName,
        number = "1",
        numberSort = 1f,
        releaseDate = releaseBase,
        bookId = persistedBook.id,
        createdDate = benchmarkDateTime,
        lastModifiedDate = benchmarkDateTime,
      ),
    )

    return library
  }

  fun ensureImportBenchmarkData(): Pair<Path, Series> {
    ensureAdminUser()

    val sourceDir = benchmarkRoot.resolve(importSourceName)
    val sourceFile = sourceDir.resolve("source.cbz")
    Files.createDirectories(sourceDir)
    createBenchmarkBookArchive(sourceFile)
    setFixedLastModified(sourceDir)
    setFixedLastModified(sourceFile)

    val library =
      libraryRepository.findAll().firstOrNull { it.name == importLibraryName }
        ?: Library(
          name = importLibraryName,
          root = benchmarkRoot.resolve("import-root").toUri().toURL(),
          createdDate = benchmarkDateTime,
          lastModifiedDate = benchmarkDateTime,
        ).also(libraryRepository::insert)

    val seriesRoot = library.path.resolve("series-1")
    Files.createDirectories(seriesRoot)
    setFixedLastModified(seriesRoot)

    val series =
      seriesRepository.findAllByLibraryId(library.id).firstOrNull { it.name == importSeriesName }
        ?: Series(
          name = importSeriesName,
          url = seriesRoot.toUri().toURL(),
          fileLastModified = benchmarkDateTime,
          libraryId = library.id,
          createdDate = benchmarkDateTime,
          lastModifiedDate = benchmarkDateTime,
        ).also(seriesRepository::insert)

    return sourceFile to series
  }

  private fun ensureAdminUser() {
    val adminEmail = "admin@example.org"
    val admin = userRepository.findByEmailIgnoreCaseOrNull(adminEmail)

    if (admin == null) {
      userRepository.insert(
        KomgaUser(
          email = adminEmail,
          password = "admin",
          roles = fullRoles,
        ),
      )
    } else if (admin.roles != fullRoles) {
      userRepository.update(admin.copy(roles = fullRoles))
    }
  }

  private fun ensureLibrary(index: Int): Library {
    val name = "Benchmark Library ${index + 1}"
    libraryRepository.findAll().firstOrNull { it.name == name }?.let { return it }

    val root = benchmarkRoot.resolve("library-${index + 1}")
    Files.createDirectories(root)

    val library =
      Library(
        name = name,
        root = root.toUri().toURL(),
        createdDate = LocalDateTime.of(2024, 1, 1, 0, 0).plusDays(index.toLong()),
        lastModifiedDate = LocalDateTime.of(2024, 1, 1, 0, 0).plusDays(index.toLong()),
      )
    libraryRepository.insert(library)
    return library
  }

  private fun ensureSeries(
    library: Library,
    libraryIndex: Int,
    seriesIndex: Int,
  ): Series {
    val name = "Benchmark Library ${libraryIndex + 1} Series ${seriesIndex + 1}"
    seriesRepository.findAllByLibraryId(library.id).firstOrNull { it.name == name }?.let { return it }

    val seriesRoot = library.path.resolve("series-${seriesIndex + 1}")
    Files.createDirectories(seriesRoot)

    val series =
      Series(
        name = name,
        url = seriesRoot.toUri().toURL(),
        fileLastModified = LocalDateTime.of(2024, 1, 1, 0, 0).plusDays((libraryIndex * 10L) + seriesIndex.toLong()),
        libraryId = library.id,
        createdDate = LocalDateTime.of(2024, 1, 1, 0, 0).plusDays((libraryIndex * 10L) + seriesIndex.toLong()),
        lastModifiedDate = LocalDateTime.of(2024, 1, 1, 0, 0).plusDays((libraryIndex * 10L) + seriesIndex.toLong()),
      )
    return seriesLifecycle.createSeries(series)
  }

  private fun ensureBooks(
    series: Series,
    library: Library,
    libraryIndex: Int,
    seriesIndex: Int,
    bookCount: Int,
    releaseOffsetStart: Long,
  ): Long {
    val seriesRoot = Path.of(series.url.toURI())
    Files.createDirectories(seriesRoot)

    val existingBooks = bookRepository.findAllBySeriesId(series.id).sortedBy { it.name }
    val existingNames = existingBooks.map { it.name }.toSet()

    val desiredBooks =
      (1..bookCount).map { number ->
        val bookName = "Benchmark Library ${libraryIndex + 1} Series ${seriesIndex + 1} Book ${number.toString().padStart(4, '0')}"
        val bookPath = seriesRoot.resolve("book-${number.toString().padStart(4, '0')}.cbz")
        Files.createDirectories(bookPath.parent)
        if (Files.notExists(bookPath)) {
          Files.createFile(bookPath)
        }
        if (libraryIndex == 0 && seriesIndex == 0 && number == 1) {
          createBenchmarkBookArchive(bookPath)
        }

        Book(
          name = bookName,
          url = bookPath.toUri().toURL(),
          fileLastModified = LocalDateTime.of(2024, 1, 1, 0, 0).plusDays((libraryIndex * 100L) + (seriesIndex * 10L) + number.toLong()),
          fileSize = 0,
          number = number,
          libraryId = library.id,
          createdDate = LocalDateTime.of(2024, 1, 1, 0, 0).plusDays((libraryIndex * 100L) + (seriesIndex * 10L) + number.toLong()),
          lastModifiedDate = LocalDateTime.of(2024, 1, 1, 0, 0).plusDays((libraryIndex * 100L) + (seriesIndex * 10L) + number.toLong()),
        )
      }

    val missingBooks = desiredBooks.filterNot { existingNames.contains(it.name) }
    if (missingBooks.isNotEmpty()) {
      seriesLifecycle.addBooks(series, missingBooks)
    }

    val allBooks = bookRepository.findAllBySeriesId(series.id).sortedBy { it.name }
    seriesLifecycle.sortBooks(series)

    val benchmarkPages = if (libraryIndex == 0 && seriesIndex == 0) createBenchmarkBookArchive(seriesRoot.resolve("book-0001.cbz")) else emptyList()

    val mediaUpdates = mutableListOf<Media>()
    val metadataUpdates = mutableListOf<BookMetadata>()
    var releaseOffset = releaseOffsetStart

    allBooks.forEach { book ->
      mediaUpdates +=
        mediaRepository.findById(book.id).copy(
          status = Media.Status.READY,
          mediaType = mediaType,
          pageCount = pageCount,
          pages = if (book.number == 1 && benchmarkPages.isNotEmpty()) benchmarkPages else emptyList(),
        )
      metadataUpdates +=
        bookMetadataRepository.findById(book.id).copy(
          releaseDate = releaseBase.plusDays(releaseOffset),
        )
      releaseOffset += 1
    }

    mediaUpdates.forEach { mediaRepository.update(it) }
    bookMetadataRepository.update(metadataUpdates)

    return releaseOffset
  }

  private fun createBenchmarkBookArchive(bookPath: Path): List<BookPage> {
    val pageBytes =
      ByteArrayOutputStream().use { output ->
        val image = BufferedImage(1, 1, BufferedImage.TYPE_INT_RGB)
        image.setRGB(0, 0, 0xFFFFFF)
        ImageIO.write(image, "png", output)
        output.toByteArray()
      }
    val pages =
      (1..pageCount).map { index ->
        BookPage(
          fileName = "page-${index.toString().padStart(4, '0')}.png",
          mediaType = "image/png",
          dimension = Dimension(1, 1),
          fileSize = pageBytes.size.toLong(),
        )
      }

    ZipOutputStream(Files.newOutputStream(bookPath)).use { zip ->
      pages.forEach { page ->
        val entry = ZipEntry(page.fileName)
        entry.time = 0L
        zip.putNextEntry(entry)
        zip.write(pageBytes)
        zip.closeEntry()
      }
    }

    return pages
  }

  private fun setFixedLastModified(path: Path) {
    Files.setLastModifiedTime(path, FileTime.from(benchmarkInstant))
  }

  private fun repairMediaDeliveryFixture() {
    val library = libraryRepository.findAll().firstOrNull { it.name == "Benchmark Library 1" } ?: return
    val series = seriesRepository.findAllByLibraryId(library.id).firstOrNull { it.name == "Benchmark Library 1 Series 1" } ?: return
    val book = bookRepository.findAllBySeriesId(series.id).firstOrNull { it.name == "Benchmark Library 1 Series 1 Book 0001" } ?: return

    val pages = createBenchmarkBookArchive(Path.of(book.url.toURI()))
    mediaRepository.update(
      mediaRepository.findById(book.id).copy(
        status = Media.Status.READY,
        mediaType = mediaType,
        pageCount = pageCount,
        pages = pages,
      ),
    )
  }
}
