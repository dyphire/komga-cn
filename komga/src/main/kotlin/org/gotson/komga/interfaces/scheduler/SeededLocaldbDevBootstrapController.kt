package org.gotson.komga.interfaces.scheduler

import io.github.oshai.kotlinlogging.KotlinLogging
import org.gotson.komga.domain.model.Book
import org.gotson.komga.domain.model.BookPage
import org.gotson.komga.domain.model.Dimension
import org.gotson.komga.domain.model.Library
import org.gotson.komga.domain.model.Media
import org.gotson.komga.domain.model.MediaType
import org.gotson.komga.domain.model.Series
import org.gotson.komga.domain.persistence.BookMetadataRepository
import org.gotson.komga.domain.persistence.BookRepository
import org.gotson.komga.domain.persistence.LibraryRepository
import org.gotson.komga.domain.persistence.MediaRepository
import org.gotson.komga.domain.persistence.SeriesMetadataRepository
import org.gotson.komga.domain.persistence.SeriesRepository
import org.gotson.komga.domain.service.SeriesLifecycle
import org.springframework.beans.factory.annotation.Value
import org.springframework.boot.context.event.ApplicationReadyEvent
import org.springframework.context.event.EventListener
import org.springframework.context.annotation.Profile
import org.springframework.data.repository.findByIdOrNull
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

private val seededLocaldbLogger = KotlinLogging.logger {}

private const val SEEDED_LIBRARY_ID = "1"
private const val SEEDED_LIBRARY_NAME = "default"
private const val SEEDED_SERIES_ID = "series-1"
private const val SEEDED_SERIES_NAME = "series"
private const val SEEDED_BOOK_ID = "book-1"
private const val SEEDED_BOOK_NAME = "book.cbr"
private const val SEEDED_PAGE_NAME = "page-0001.png"
private val seededLocaldbDateTime = LocalDateTime.parse("2024-01-02T03:04:05")
private val seededLocaldbReleaseDate = LocalDate.of(2024, 1, 1)
private val seededLocaldbInstant = Instant.parse("2024-01-02T03:04:05Z")

@Profile("dev & noclaim")
@Component
class SeededLocaldbDevBootstrapController(
  private val libraryRepository: LibraryRepository,
  private val seriesRepository: SeriesRepository,
  private val seriesMetadataRepository: SeriesMetadataRepository,
  private val bookRepository: BookRepository,
  private val mediaRepository: MediaRepository,
  private val bookMetadataRepository: BookMetadataRepository,
  private val seriesLifecycle: SeriesLifecycle,
  @Value("\${komga.config-dir}") private val configDir: String,
) {
  @EventListener(ApplicationReadyEvent::class)
  fun ensureSeededLocaldbFixture() {
    val seededRoot = Path.of(configDir).resolve("seeded-localdb").resolve("library1")
    val seriesRoot = seededRoot.resolve(SEEDED_SERIES_NAME)
    val bookPath = seriesRoot.resolve(SEEDED_BOOK_NAME)
    val pageSize = writeSeededBookArchive(bookPath)

    val library = ensureLibrary(seededRoot)
    val series = ensureSeries(library, seriesRoot)
    val book = ensureBook(library, series, bookPath)

    mediaRepository.update(
      mediaRepository.findById(book.id).copy(
        status = Media.Status.READY,
        mediaType = MediaType.ZIP.type,
        pageCount = 1,
        pages = listOf(BookPage(SEEDED_PAGE_NAME, "image/png", Dimension(1, 1), fileSize = pageSize.toLong())),
        lastModifiedDate = seededLocaldbDateTime,
      ),
    )

    seriesMetadataRepository.update(
      seriesMetadataRepository.findById(series.id).copy(
        title = SEEDED_SERIES_NAME,
        titleSort = SEEDED_SERIES_NAME,
        lastModifiedDate = seededLocaldbDateTime,
      ),
    )

    bookMetadataRepository.update(
      bookMetadataRepository.findById(book.id).copy(
        title = SEEDED_BOOK_NAME,
        number = "1",
        numberSort = 1f,
        releaseDate = seededLocaldbReleaseDate,
        lastModifiedDate = seededLocaldbDateTime,
      ),
    )

    seededLocaldbLogger.info { "Ensured dev seeded-localdb fixture for $SEEDED_BOOK_ID at $bookPath" }
  }

  private fun ensureLibrary(root: Path): Library {
    val desired =
      Library(
        id = SEEDED_LIBRARY_ID,
        name = SEEDED_LIBRARY_NAME,
        root = root.toUri().toURL(),
        createdDate = seededLocaldbDateTime,
        lastModifiedDate = seededLocaldbDateTime,
      )

    return libraryRepository.findByIdOrNull(SEEDED_LIBRARY_ID)?.let {
      it.copy(
        name = desired.name,
        root = desired.root,
        lastModifiedDate = desired.lastModifiedDate,
      ).also(libraryRepository::update)
    } ?: desired.also(libraryRepository::insert)
  }

  private fun ensureSeries(
    library: Library,
    seriesRoot: Path,
  ): Series {
    val desired =
      Series(
        id = SEEDED_SERIES_ID,
        name = SEEDED_SERIES_NAME,
        url = seriesRoot.toUri().toURL(),
        fileLastModified = seededLocaldbDateTime,
        libraryId = library.id,
        createdDate = seededLocaldbDateTime,
        lastModifiedDate = seededLocaldbDateTime,
      )

    return seriesRepository.findByIdOrNull(SEEDED_SERIES_ID)?.let {
      it.copy(
        name = desired.name,
        url = desired.url,
        fileLastModified = desired.fileLastModified,
        libraryId = desired.libraryId,
        lastModifiedDate = desired.lastModifiedDate,
      ).also(seriesRepository::update)
    } ?: seriesLifecycle.createSeries(desired)
  }

  private fun ensureBook(
    library: Library,
    series: Series,
    bookPath: Path,
  ): Book {
    val desired =
      Book(
        id = SEEDED_BOOK_ID,
        name = SEEDED_BOOK_NAME,
        url = bookPath.toUri().toURL(),
        fileLastModified = seededLocaldbDateTime,
        fileSize = Files.size(bookPath),
        number = 1,
        seriesId = series.id,
        libraryId = library.id,
        createdDate = seededLocaldbDateTime,
        lastModifiedDate = seededLocaldbDateTime,
      )

    return bookRepository.findByIdOrNull(SEEDED_BOOK_ID)?.let {
      it.copy(
        name = desired.name,
        url = desired.url,
        fileLastModified = desired.fileLastModified,
        fileSize = desired.fileSize,
        number = desired.number,
        seriesId = desired.seriesId,
        libraryId = desired.libraryId,
        lastModifiedDate = desired.lastModifiedDate,
      ).also(bookRepository::update)
    } ?: run {
      seriesLifecycle.addBooks(series, listOf(desired))
      bookRepository.findByIdOrNull(SEEDED_BOOK_ID)!!
    }
  }

  private fun writeSeededBookArchive(bookPath: Path): Int {
    Files.createDirectories(bookPath.parent)

    val pageBytes =
      ByteArrayOutputStream().use { output ->
        val image = BufferedImage(1, 1, BufferedImage.TYPE_INT_RGB)
        image.setRGB(0, 0, 0xFFFFFF)
        ImageIO.write(image, "png", output)
        output.toByteArray()
      }

    ZipOutputStream(Files.newOutputStream(bookPath)).use { zip ->
      val entry = ZipEntry(SEEDED_PAGE_NAME)
      entry.time = 0L
      zip.putNextEntry(entry)
      zip.write(pageBytes)
      zip.closeEntry()
    }

    setFixedLastModified(bookPath.parent.parent)
    setFixedLastModified(bookPath.parent)
    setFixedLastModified(bookPath)

    return pageBytes.size
  }

  private fun setFixedLastModified(path: Path) {
    Files.setLastModifiedTime(path, FileTime.from(seededLocaldbInstant))
  }
}
