package org.gotson.komga.domain.service

import io.github.oshai.kotlinlogging.KotlinLogging
import kotlinx.coroutines.*
import org.gotson.komga.domain.model.Book
import org.gotson.komga.domain.model.BookPage
import org.gotson.komga.domain.model.BookWithMedia
import org.gotson.komga.domain.model.Dimension
import org.gotson.komga.domain.model.Media
import org.gotson.komga.domain.model.MediaExtensionEpub
import org.gotson.komga.domain.model.MediaFile
import org.gotson.komga.domain.model.MediaNotReadyException
import org.gotson.komga.domain.model.MediaProfile
import org.gotson.komga.domain.model.MediaProfile.*
import org.gotson.komga.domain.model.MediaType
import org.gotson.komga.domain.model.MediaUnsupportedException
import org.gotson.komga.domain.model.NoThumbnailFoundException
import org.gotson.komga.domain.model.ThumbnailBook
import org.gotson.komga.domain.model.TypedBytes
import org.gotson.komga.infrastructure.configuration.KomgaProperties
import org.gotson.komga.infrastructure.configuration.KomgaSettingsProvider
import org.gotson.komga.infrastructure.hash.Hasher
import org.gotson.komga.infrastructure.image.ImageAnalyzer
import org.gotson.komga.infrastructure.image.ImageConverter
import org.gotson.komga.infrastructure.image.ImageType
import org.gotson.komga.infrastructure.image.QrCodeDetector
import org.gotson.komga.infrastructure.mediacontainer.ContentDetector
import org.gotson.komga.infrastructure.mediacontainer.divina.DivinaExtractor
import org.gotson.komga.infrastructure.mediacontainer.epub.EpubExtractor
import org.gotson.komga.infrastructure.mediacontainer.epub.epub
import org.gotson.komga.infrastructure.mediacontainer.mobi.MobiExtractor
import org.gotson.komga.infrastructure.mediacontainer.pdf.PdfExtractor
import io.micrometer.core.annotation.Timed
import org.springframework.beans.factory.annotation.Qualifier
import org.springframework.beans.factory.annotation.Value
import org.springframework.stereotype.Service
import com.github.benmanes.caffeine.cache.Caffeine
import java.io.ByteArrayOutputStream
import java.nio.file.AccessDeniedException
import java.nio.file.NoSuchFileException
import java.util.concurrent.TimeUnit
import javax.imageio.ImageIO
import kotlin.io.path.extension

private val logger = KotlinLogging.logger {}

@Service
class BookAnalyzer(
  private val contentDetector: ContentDetector,
  extractors: List<DivinaExtractor>,
  private val pdfExtractor: PdfExtractor,
  private val epubExtractor: EpubExtractor,
  private val mobiExtractor: MobiExtractor,
  private val imageConverter: ImageConverter,
  private val imageAnalyzer: ImageAnalyzer,
  private val qrCodeDetector: QrCodeDetector,
  private val hasher: Hasher,
  @param:Value("#{@komgaProperties.pageHashing}") private val pageHashing: Int,
  private val komgaSettingsProvider: KomgaSettingsProvider,
  private val komgaProperties: KomgaProperties,
  @Qualifier("thumbnailType")
  private val thumbnailType: ImageType,
  @Qualifier("pdfImageType")
  private val pdfImageType: ImageType,
) {
  val divinaExtractors =
    extractors
      .flatMap { e -> e.mediaTypes().map { it to e } }
      .toMap()

  // Cache optimization: media type detection cache
  private val mediaTypeCache = Caffeine.newBuilder()
    .maximumSize(komgaProperties.analysis.mediaTypeCacheSize.toLong())
    .expireAfterWrite(komgaProperties.analysis.mediaTypeCacheExpireHours.toHours(), TimeUnit.HOURS)
    .build<String, MediaType>()

  // Image analysis result cache
  private val imageAnalysisCache = Caffeine.newBuilder()
    .maximumSize(komgaProperties.analysis.imageAnalysisCacheSize.toLong())
    .expireAfterWrite(komgaProperties.analysis.imageAnalysisCacheExpireMinutes.toMinutes(), TimeUnit.MINUTES)
    .build<String, ImageAnalysisResult>()

  // Internal data class for caching image analysis results
  private data class ImageAnalysisResult(
    val isSuitable: Boolean,
    val timestamp: Long = System.currentTimeMillis()
  )

  @Timed("book.analyze")
  fun analyze(
    book: Book,
    analyzeDimensions: Boolean,
    adPagesDetector: Boolean,
  ): Media {
    logger.info { "Analyzing book: $book" }
    val startTime = System.nanoTime()

    return try {
      // Use cache to optimize media type detection
      val cacheKey = book.path.toString()
      var mediaType = mediaTypeCache.getIfPresent(cacheKey)

      if (mediaType == null) {
        mediaType = contentDetector.detectMediaType(book.path).let {
          logger.info { "Detected media type: $it" }
          MediaType.fromMediaType(it) ?: return Media(mediaType = it, status = Media.Status.UNSUPPORTED, comment = "ERR_1001", bookId = book.id)
        }
        mediaTypeCache.put(cacheKey, mediaType)
      } else {
        logger.debug { "Using cached media type: $mediaType for book: $book" }
      }

      if (book.path.extension.lowercase() == "epub" && mediaType != MediaType.EPUB) {
        if (epubExtractor.isEpub(book.path)) {
          mediaType = MediaType.EPUB
        } else {
          logger.warn { "Epub file is malformed, file is probably broken: ${book.path}" }
          return Media(mediaType = mediaType.type, status = Media.Status.ERROR, comment = "ERR_1032", bookId = book.id)
        }
      }

      val media = when (mediaType.profile) {
        DIVINA -> analyzeDivina(book, mediaType, analyzeDimensions, adPagesDetector)
        PDF -> analyzePdf(book, analyzeDimensions)
        EPUB -> analyzeEpub(book, analyzeDimensions, adPagesDetector)
        MOBI -> analyzeMobi(book, analyzeDimensions)
      }.copy(mediaType = mediaType.type)

      val duration = (System.nanoTime() - startTime) / 1_000_000 // Convert to milliseconds
      logger.info { "Book ${book.id} analyzed successfully in ${duration}ms" }
      media

    } catch (ade: AccessDeniedException) {
      val duration = (System.nanoTime() - startTime) / 1_000_000
      logger.error(ade) { "Access denied while analyzing book ${book.id} (${duration}ms)" }
      Media(status = Media.Status.ERROR, comment = "ERR_1000")
    } catch (ex: NoSuchFileException) {
      val duration = (System.nanoTime() - startTime) / 1_000_000
      logger.error(ex) { "File not found while analyzing book ${book.id} (${duration}ms)" }
      Media(status = Media.Status.ERROR, comment = "ERR_1018")
    } catch (ex: Exception) {
      val duration = (System.nanoTime() - startTime) / 1_000_000
      logger.error(ex) { "Unexpected error while analyzing book ${book.id} (${duration}ms): ${ex.message}" }
      Media(status = Media.Status.ERROR, comment = "ERR_1005")
    }.copy(bookId = book.id)
  }

  private fun filterAdPages(pages: List<BookPage>, book: Book, mediaType: MediaType): List<BookPage> {
    if (pages.size <= 10) return pages

    // Use configuration parameter: check last N pages
    val pagesToCheckCount = minOf(komgaProperties.analysis.adPagesCheckCount, pages.size)
    val pagesToCheck = pages.takeLast(pagesToCheckCount)
    val pagesToKeep = pages.dropLast(pagesToCheckCount)

    // Early exit: reverse detection, exit if 3 consecutive pages have no ads
    var consecutiveNormal = 0
    val adFlags = BooleanArray(pagesToCheck.size) { false }

    // Start detection from the last page in reverse order
    for ((index, page) in pagesToCheck.reversed().withIndex()) {
      val isAd = try {
        val content = divinaExtractors.getValue(mediaType.type)
            .getEntryStream(book.path, page.fileName)
        qrCodeDetector.containsQrCode(content)
      } catch (e: Exception) {
        logger.error(e) { "Error while checking QR code for page: ${page.fileName} in book: $book" }
        false
      }

      adFlags[pagesToCheck.size - 1 - index] = isAd

      if (!isAd) {
        consecutiveNormal++
        if (consecutiveNormal >= 3) {
          return pages
        }
      } else {
        consecutiveNormal = 0
      }
    }

    var consecutiveAd = 0
    for (i in adFlags.indices) {
      if (adFlags[i]) {
        consecutiveAd++
        continue
      }

      if (consecutiveAd >= 2) {
        adFlags[i] = true
      } else if ((i - 1 >= 0 && adFlags[i - 1]) && (i + 1 < adFlags.size && adFlags[i + 1])) {
        adFlags[i] = true
      } else {
        consecutiveAd = 0
      }
    }

    val filteredLastPages = pagesToCheck.filterIndexed { index, _ -> !adFlags[index] }
    return pagesToKeep + filteredLastPages
  }

  private fun analyzeMobi(
    book: Book,
    analyzeDimensions: Boolean,
  ): Media {
    val pages = mobiExtractor.getPages(book.path, analyzeDimensions).map { BookPage(it.name, "", it.dimension) }
    return Media(status = Media.Status.READY, pages = pages)
  }

  private fun analyzeDivina(
    book: Book,
    mediaType: MediaType,
    analyzeDimensions: Boolean,
    adPagesDetector: Boolean,
  ): Media {
    val entries =
      try {
        divinaExtractors[mediaType.type]?.getEntries(book.path, analyzeDimensions)
          ?: return Media(status = Media.Status.UNSUPPORTED)
      } catch (ex: MediaUnsupportedException) {
        return Media(status = Media.Status.UNSUPPORTED, comment = ex.code)
      } catch (ex: Exception) {
        logger.error(ex) { "Error while analyzing book: $book" }
        return Media(status = Media.Status.ERROR, comment = "ERR_1008")
      }

    val (pages, others) =
      entries
        .partition { entry ->
          entry.mediaType?.let { contentDetector.isImage(it) } ?: false
        }.let { (images, others) ->
          Pair(
            images.map { BookPage(fileName = it.name, mediaType = it.mediaType!!, dimension = it.dimension, fileSize = it.fileSize) },
            others,
          )
        }

    val filteredPages = if (adPagesDetector) {
        filterAdPages(pages, book, mediaType)
      } else {
        pages
      }

    val entriesErrorSummary =
      others
        .filter { it.mediaType.isNullOrBlank() }
        .map { it.name }
        .ifEmpty { null }
        ?.joinToString(prefix = "ERR_1007 [", postfix = "]") { it }

    if (filteredPages.isEmpty()) {
      logger.warn { "Book $book does not contain any pages after QR code filtering" }
      return Media(status = Media.Status.ERROR, comment = "ERR_1006")
    }

    val removedCount = pages.size - filteredPages.size
    if (removedCount > 0) {
      logger.info { "Removed $removedCount pages containing QR codes from book: $book" }
    }

    val files = others.map { MediaFile(fileName = it.name, mediaType = it.mediaType, fileSize = it.fileSize) }

    return Media(status = Media.Status.READY, pages = filteredPages, pageCount = filteredPages.size, files = files, comment = entriesErrorSummary)
  }

  private fun analyzeEpub(
    book: Book,
    analyzeDimensions: Boolean,
    adPagesDetector: Boolean,
  ): Media {
    book.path.epub { epub ->
      val (resources, missingResources) = epubExtractor.getResources(epub).partition { it.fileSize != null }
      val isFixedLayout = epubExtractor.isFixedLayout(epub)
      val isKepub = epubExtractor.isKepub(epub, resources)

      val errors = mutableListOf<String>()

      val toc =
        try {
          epubExtractor.getToc(epub)
        } catch (e: Exception) {
          logger.error(e) { "Error while getting EPUB TOC" }
          errors.add("ERR_1035")
          emptyList()
        }

      val landmarks =
        try {
          epubExtractor.getLandmarks(epub)
        } catch (e: Exception) {
          logger.error(e) { "Error while getting EPUB Landmarks" }
          errors.add("ERR_1036")
          emptyList()
        }

      val pageList =
        try {
          epubExtractor.getPageList(epub)
        } catch (e: Exception) {
          logger.error(e) { "Error while getting EPUB page list" }
          errors.add("ERR_1037")
          emptyList()
        }

      val divinaPages =
        try {
          epubExtractor.getDivinaPages(epub, isFixedLayout, analyzeDimensions)
        } catch (e: Exception) {
          logger.error(e) { "Error while getting EPUB Divina pages" }
          errors.add("ERR_1038")
          emptyList()
        }

      val filteredDivinaPages = if (adPagesDetector) {
          filterAdPages(divinaPages, book, MediaType.EPUB)
        } else {
          divinaPages
        }

      val positions =
        try {
          epubExtractor.computePositions(epub, book.path, resources, isFixedLayout, isKepub)
        } catch (e: Exception) {
          logger.error(e) { "Error while getting EPUB positions" }
          errors.add("ERR_1039")
          emptyList()
        }

      val entriesErrorSummary =
        missingResources
          .map { it.fileName }
          .ifEmpty { null }
          ?.joinToString(prefix = "ERR_1033 [", postfix = "]") { it }

      val allErrors =
        (errors + entriesErrorSummary)
          .filterNotNull()
          .joinToString(" ")
          .ifBlank { null }

      return Media(
        status = Media.Status.READY,
        pages = filteredDivinaPages,
        files = resources,
        pageCount = epubExtractor.computePageCount(epub),
        epubDivinaCompatible = filteredDivinaPages.isNotEmpty(),
        epubIsKepub = isKepub,
        extension =
          MediaExtensionEpub(
            toc = toc,
            landmarks = landmarks,
            pageList = pageList,
            isFixedLayout = isFixedLayout,
            positions = positions,
          ),
        comment = allErrors,
      )
    }
  }

  private fun analyzePdf(
    book: Book,
    analyzeDimensions: Boolean,
  ): Media {
    val pages = pdfExtractor.getPages(book.path, analyzeDimensions).map { BookPage(it.name, "", it.dimension) }
    return Media(status = Media.Status.READY, pages = pages)
  }

  @Throws(
    MediaNotReadyException::class,
    NoThumbnailFoundException::class,
  )
  fun generateThumbnail(book: BookWithMedia): ThumbnailBook {
    logger.info { "Generate thumbnail for book: $book" }

    if (book.media.status != Media.Status.READY) {
      logger.warn { "Book media is not ready, cannot generate thumbnail. Book: $book" }
      throw MediaNotReadyException()
    }

    val thumbnail =
      getPoster(book)?.let { cover ->
        imageConverter.resizeImageToByteArray(cover.bytes, thumbnailType, komgaSettingsProvider.thumbnailSize.maxEdge)
      } ?: throw NoThumbnailFoundException()

    return ThumbnailBook(
      thumbnail = thumbnail,
      type = ThumbnailBook.Type.GENERATED,
      bookId = book.book.id,
      mediaType = thumbnailType.mediaType,
      dimension = imageAnalyzer.getDimension(thumbnail.inputStream()) ?: Dimension(0, 0),
      fileSize = thumbnail.size.toLong(),
    )
  }

  fun getPoster(book: BookWithMedia): TypedBytes? =
    when (book.media.profile) {
      DIVINA -> getDivinaPoster(book)
      PDF -> pdfExtractor.getPageContentAsImage(book.book.path, 1)
      EPUB -> epubExtractor.getCover(book.book.path)
      MOBI -> mobiExtractor.getCover(book.book.path)
      null -> null
    }

  private fun getDivinaPoster(book: BookWithMedia): TypedBytes? {
    val extractor = divinaExtractors[book.media.mediaType] ?: return null

    // Try up to first 3 pages to find a suitable cover
    for (pageIndex in 0 until minOf(3, book.media.pages.size)) {
      val page = book.media.pages[pageIndex]
      val pageData = extractor.getEntryStream(book.book.path, page.fileName) ?: continue

      // Check if this page is suitable as cover (not mostly white/black)
      if (isSuitableCoverImage(pageData)) {
        return TypedBytes(pageData, page.mediaType)
      }
    }

    // If no suitable page found, return first page as fallback
    return book.media.pages.firstOrNull()?.let { firstPage ->
      extractor.getEntryStream(book.book.path, firstPage.fileName)?.let {
        TypedBytes(it, firstPage.mediaType)
      }
    }
  }

  private fun isSuitableCoverImage(imageData: ByteArray): Boolean {
    // Use configuration parameter to control memory limit
    val MAX_IMAGE_SIZE = komgaProperties.analysis.maxImageSizeForAnalysis
    if (imageData.size > MAX_IMAGE_SIZE) {
      logger.debug { "Skipping cover analysis for large image (${imageData.size} bytes)" }
      return true // Assume large images are suitable
    }

    // Use cache to optimize image analysis
    val cacheKey = hasher.computeHash(imageData.inputStream())
    val cachedResult = imageAnalysisCache.getIfPresent(cacheKey)
    if (cachedResult != null) {
      logger.debug { "Using cached image analysis result for hash: $cacheKey" }
      return cachedResult.isSuitable
    }

    // Use coroutine for async processing to avoid blocking main thread
    val result = runBlocking {
      withContext(Dispatchers.IO) {
        analyzeCoverImageAsync(imageData)
      }
    }

    // Cache result
    imageAnalysisCache.put(cacheKey, ImageAnalysisResult(result))
    return result
  }

  private fun analyzeCoverImageAsync(imageData: ByteArray): Boolean {
    var image: java.awt.image.BufferedImage? = null
    return try {
      // Use stream-based reading to avoid loading entire image at once if possible
      imageData.inputStream().use { stream ->
        val imageInputStream = javax.imageio.ImageIO.createImageInputStream(stream)
        if (imageInputStream == null) {
          // Fallback to direct reading
          image = javax.imageio.ImageIO.read(stream)
        } else {
          // Try to find a reader that supports streaming
          val readers = javax.imageio.ImageIO.getImageReaders(imageInputStream)
          if (readers.hasNext()) {
            val reader = readers.next()
            reader.input = imageInputStream
            image = reader.read(0) // Read first frame only
            reader.dispose()
          } else {
            // Fallback
            image = javax.imageio.ImageIO.read(stream)
          }
          imageInputStream.close()
        }
      }

      val bufferedImage = image ?: return false

      val width = bufferedImage.width
      val height = bufferedImage.height

      // Skip extremely large images
      val MAX_DIMENSION = 5000
      if (width > MAX_DIMENSION || height > MAX_DIMENSION) {
        logger.debug { "Skipping cover analysis for very large image (${width}x${height})" }
        return true
      }

      var whitePixels = 0
      var blackPixels = 0

      // More aggressive sampling: sample every 50th pixel
      val sampleStep = maxOf(1, minOf(width, height) / 50) // More aggressive sampling
      val sampledPixels = ((width + sampleStep - 1) / sampleStep) * ((height + sampleStep - 1) / sampleStep)

      for (y in 0 until height step sampleStep) {
        for (x in 0 until width step sampleStep) {
          val rgb = bufferedImage.getRGB(x, y)
          val r = (rgb shr 16) and 0xFF
          val g = (rgb shr 8) and 0xFF
          val b = rgb and 0xFF

          // Consider pixel as white if RGB values are all > 240
          if (r > 240 && g > 240 && b > 240) {
            whitePixels++
          }
          // Consider pixel as black if RGB values are all < 15
          else if (r < 15 && g < 15 && b < 15) {
            blackPixels++
          }
        }
      }

      val whiteRatio = whitePixels.toDouble() / sampledPixels
      val blackRatio = blackPixels.toDouble() / sampledPixels

      // Suitable if neither white nor black pixels exceed 95%
      whiteRatio < 0.95 && blackRatio < 0.95
    } catch (e: Exception) {
      logger.warn(e) { "Error analyzing cover image, considering it unsuitable" }
      false
    } finally {
      // Manual cleanup to help GC
      image?.flush()
      image = null
    }
  }

  @Throws(
    MediaNotReadyException::class,
    IndexOutOfBoundsException::class,
  )
  fun getPageContent(
    book: BookWithMedia,
    number: Int,
  ): ByteArray {
    logger.debug { "Get page #$number for book: $book" }

    if (book.media.status != Media.Status.READY) {
      logger.warn { "Book media is not ready, cannot get pages" }
      throw MediaNotReadyException()
    }

    if (number > book.media.pageCount || number <= 0) {
      logger.error { "Page number #$number is out of bounds. Book has ${book.media.pageCount} pages" }
      throw IndexOutOfBoundsException("Page $number does not exist")
    }

    return when (book.media.profile) {
      DIVINA -> divinaExtractors.getValue(book.media.mediaType!!).getEntryStream(book.book.path, book.media.pages[number - 1].fileName)
      PDF -> pdfExtractor.getPageContentAsImage(book.book.path, number).bytes
      EPUB ->
        if (book.media.epubDivinaCompatible)
          epubExtractor.getEntryStream(book.book.path, book.media.pages[number - 1].fileName)
        else
          throw MediaUnsupportedException("Epub profile does not support getting page content")

      MOBI -> mobiExtractor.getPageContentAsImage(book.book.path, number).bytes
      null -> throw MediaNotReadyException()
    }
  }

  @Throws(
    MediaNotReadyException::class,
    IndexOutOfBoundsException::class,
  )
  fun getPageContentRaw(
    book: BookWithMedia,
    number: Int,
  ): TypedBytes {
    logger.debug { "Get raw page #$number for book: $book" }

    if (book.media.profile == MOBI) {
      return mobiExtractor.getPageContentAsImage(book.book.path, number)
    }

    if (book.media.profile != MediaProfile.PDF) throw MediaUnsupportedException("Extractor does not support raw extraction of pages")

    if (book.media.status != Media.Status.READY) {
      logger.warn { "Book media is not ready, cannot get pages" }
      throw MediaNotReadyException()
    }

    if (number > book.media.pageCount || number <= 0) {
      logger.error { "Page number #$number is out of bounds. Book has ${book.media.pageCount} pages" }
      throw IndexOutOfBoundsException("Page $number does not exist")
    }

    return pdfExtractor.getPageContentAsPdf(book.book.path, number)
  }

  @Throws(
    MediaNotReadyException::class,
  )
  fun getFileContent(
    book: BookWithMedia,
    fileName: String,
  ): ByteArray {
    logger.debug { "Get file $fileName for book: $book" }

    if (book.media.status != Media.Status.READY) {
      logger.warn { "Book media is not ready, cannot get files" }
      throw MediaNotReadyException()
    }

    return when (book.media.profile) {
      DIVINA -> divinaExtractors.getValue(book.media.mediaType!!).getEntryStream(book.book.path, fileName)
      EPUB -> epubExtractor.getEntryStream(book.book.path, fileName)
      PDF, MOBI, null -> throw MediaUnsupportedException("Extractor does not support extraction of files")
    }
  }

  /**
   * Will hash the first and last pages of the given book.
   * The number of pages hashed from start/end is configurable.
   *
   * See [org.gotson.komga.infrastructure.configuration.KomgaProperties.pageHashing]
   */
  fun hashPages(book: BookWithMedia): Media {
    val hashedPages =
      book.media.pages.mapIndexed { index, bookPage ->
        if (bookPage.fileHash.isBlank() && (index < pageHashing || index >= (book.media.pageCount - pageHashing))) {
          val content = getPageContent(book, index + 1)
          val hash = hashPage(bookPage, content)
          bookPage.copy(fileHash = hash)
        } else {
          bookPage
        }
      }

    return book.media.copy(pages = hashedPages)
  }

  /**
   * Hash a single page, using the file content for hashing.
   *
   * For JPEG, the image is read/written to remove the metadata.
   */
  fun hashPage(
    page: BookPage,
    content: ByteArray,
  ): String {
    val bytes =
      if (page.mediaType == ImageType.JPEG.mediaType) {
        // JPEG could contain different EXIF data, reading and writing back the image will get rid of it
        ByteArrayOutputStream().use { buffer ->
          ImageIO.write(ImageIO.read(content.inputStream()), ImageType.JPEG.imageIOFormat, buffer)
          buffer.toByteArray()
        }
      } else {
        content
      }

    return hasher.computeHash(bytes.inputStream())
  }

  fun getPdfPagesDynamic(media: Media): List<BookPage> {
    if (media.profile != PDF) throw MediaUnsupportedException("Cannot get synthetic pages for non-PDF media")

    return media.pages.map { page ->
      page.copy(
        mediaType = pdfImageType.mediaType,
        dimension = page.dimension?.let { pdfExtractor.scaleDimension(it) },
      )
    }
  }
}
