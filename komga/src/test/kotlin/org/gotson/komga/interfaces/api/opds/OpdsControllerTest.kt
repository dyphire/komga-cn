package org.gotson.komga.interfaces.api.opds

import org.gotson.komga.domain.model.BookPage
import org.gotson.komga.domain.model.Book
import org.gotson.komga.domain.model.KomgaUser
import org.gotson.komga.domain.model.Media
import org.gotson.komga.domain.model.Series
import org.gotson.komga.domain.model.makeLibrary
import org.gotson.komga.domain.model.makeBook
import org.gotson.komga.domain.model.makeSeries
import org.gotson.komga.domain.persistence.BookRepository
import org.gotson.komga.domain.persistence.KomgaUserRepository
import org.gotson.komga.domain.persistence.LibraryRepository
import org.gotson.komga.domain.persistence.MediaRepository
import org.gotson.komga.domain.persistence.SeriesMetadataRepository
import org.gotson.komga.domain.persistence.SeriesRepository
import org.gotson.komga.domain.service.KomgaUserLifecycle
import org.gotson.komga.domain.service.LibraryLifecycle
import org.gotson.komga.domain.service.SeriesLifecycle
import org.gotson.komga.interfaces.api.rest.WithMockCustomUser
import org.hamcrest.Matchers
import org.junit.jupiter.api.AfterAll
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.BeforeAll
import org.junit.jupiter.api.Nested
import org.junit.jupiter.api.Test
import org.springframework.beans.factory.annotation.Autowired
import org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc
import org.springframework.boot.test.context.SpringBootTest
import org.springframework.http.HttpHeaders
import org.springframework.core.io.ClassPathResource
import org.springframework.test.json.JsonCompareMode
import org.springframework.test.web.servlet.MockMvc
import org.springframework.test.web.servlet.get
import javax.sql.DataSource
import java.sql.Timestamp
import java.net.URL
import java.time.LocalDateTime

@SpringBootTest
@AutoConfigureMockMvc(printOnlyOnFailure = false)
class OpdsControllerTest(
  @Autowired private val seriesRepository: SeriesRepository,
  @Autowired private val seriesLifecycle: SeriesLifecycle,
  @Autowired private val seriesMetadataRepository: SeriesMetadataRepository,
  @Autowired private val libraryRepository: LibraryRepository,
  @Autowired private val libraryLifecycle: LibraryLifecycle,
  @Autowired private val bookRepository: BookRepository,
  @Autowired private val mediaRepository: MediaRepository,
  @Autowired private val userRepository: KomgaUserRepository,
  @Autowired private val userLifecycle: KomgaUserLifecycle,
  @Autowired private val dataSource: DataSource,
  @Autowired private val mockMvc: MockMvc,
) {
  private val library = makeLibrary(id = "1")
  private val user = KomgaUser("user@example.org", "", id = "1")
  private val user2 = KomgaUser("user2@example.org", "", id = "2")

  @BeforeAll
  fun `setup library`() {
    libraryRepository.insert(library)
    userRepository.insert(user)
    userRepository.insert(user2)
  }

  @AfterAll
  fun teardown() {
    userRepository.findAll().forEach {
      userLifecycle.deleteUser(it)
    }
    libraryRepository.findAll().forEach {
      libraryLifecycle.deleteLibrary(it)
    }
  }

  @AfterEach
  fun `clear repository`() {
    seriesLifecycle.deleteMany(seriesRepository.findAll())
  }

  private fun createReadyPdfBook(name: String = "book.pdf") =
    LocalDateTime.of(2024, 1, 2, 3, 4, 5).let { fixedDateTime ->
      val series =
        Series(
          name = "series",
          url = URL("file:/series"),
          fileLastModified = fixedDateTime,
          id = "series-1",
          libraryId = library.id,
          createdDate = fixedDateTime,
          lastModifiedDate = fixedDateTime,
        )

      val book =
        Book(
          name = name,
          url = URL("file:/$name"),
          fileLastModified = fixedDateTime,
          id = "book-1",
          seriesId = series.id,
          libraryId = library.id,
          createdDate = fixedDateTime,
          lastModifiedDate = fixedDateTime,
        )

      seriesLifecycle.createSeries(series).also { created ->
        seriesLifecycle.addBooks(created, listOf(book))
      }

      dataSource.connection.use { connection ->
        connection
          .prepareStatement("update BOOK set CREATED_DATE = ?, LAST_MODIFIED_DATE = ? where ID = ?")
          .use { statement ->
            statement.setTimestamp(1, Timestamp.valueOf(fixedDateTime))
            statement.setTimestamp(2, Timestamp.valueOf(fixedDateTime))
            statement.setString(3, book.id)
            statement.executeUpdate()
          }
      }

      mediaRepository.findById(book.id).let { media ->
        mediaRepository.update(
          media.copy(
            status = Media.Status.READY,
            mediaType = "application/pdf",
            pageCount = 1,
            pages = listOf(BookPage("1.jpg", "image/jpeg")),
          ),
        )
      }

      book
    }

  private fun readSnapshot(path: String): String =
    ClassPathResource(path).inputStream.bufferedReader().use { it.readText() }

  @Test
  fun `given unauthenticated request when getting opds v2 catalog then auth challenge is returned`() {
    mockMvc
      .get("/opds/v2/catalog")
      .andExpect {
        status { isUnauthorized() }
        header { string(HttpHeaders.WWW_AUTHENTICATE, "Basic realm=\"Realm\"") }
        header { string(HttpHeaders.LINK, Matchers.containsString("<http://localhost/opds/v2/auth>; rel=\"http://opds-spec.org/auth/document\"")) }
        header { string(HttpHeaders.CONTENT_TYPE, Matchers.containsString("application/opds-authentication+json")) }
        jsonPath("$.id") { value(Matchers.containsString("/opds/v2/auth")) }
        jsonPath("$.title") { value("Komga") }
        jsonPath("$.authentication[0].type") { value("http://opds-spec.org/auth/basic") }
        jsonPath("$.authentication[0].labels.login") { value("Email") }
        jsonPath("$.authentication[0].labels.password") { value("Password") }
      }
  }

  @Test
  fun `when getting opds v2 auth document then returns auth document`() {
    mockMvc
      .get("/opds/v2/auth")
      .andExpect {
        status { isOk() }
        header { string(HttpHeaders.CONTENT_TYPE, Matchers.containsString("application/opds-authentication+json")) }
        content {
          json(readSnapshot("compatibility-snapshots/opds/opds-v2-auth.json"), JsonCompareMode.STRICT)
        }
        jsonPath("$.id") { value(Matchers.containsString("/opds/v2/auth")) }
        jsonPath("$.title") { value("Komga") }
        jsonPath("$.description") { value("Enter your email and password to authenticate.") }
        jsonPath("$.links.length()") { value(2) }
        jsonPath("$.authentication.length()") { value(1) }
        jsonPath("$.authentication[0].type") { value("http://opds-spec.org/auth/basic") }
        jsonPath("$.authentication[0].labels.login") { value("Email") }
        jsonPath("$.authentication[0].labels.password") { value("Password") }
      }
  }

  @Test
  @WithMockCustomUser
  fun `given ready pdf book when getting opds v2 manifest then returns stable manifest shape`() {
    val book = createReadyPdfBook()

    mockMvc
      .get("/opds/v2/books/${book.id}/manifest")
      .andExpect {
        status { isOk() }
        header { string(HttpHeaders.CONTENT_TYPE, Matchers.containsString("application/opds-publication+json")) }
        content {
          json(readSnapshot("compatibility-snapshots/opds/opds-v2-manifest.json"), JsonCompareMode.STRICT)
        }
        jsonPath("$.context") { value("https://readium.org/webpub-manifest/context.jsonld") }
        jsonPath("$.metadata.title") { value(book.name) }
        jsonPath("$.links[0].rel") { value("self") }
        jsonPath("$.links[0].href") { value(Matchers.containsString("/opds/v2/books/${book.id}/manifest")) }
        jsonPath("$.links[1].href") { value(Matchers.containsString("/opds/v2/books/${book.id}/manifest/divina")) }
        jsonPath("$.links[2].rel") { value("http://opds-spec.org/acquisition") }
        jsonPath("$.links[3].rel") { value("http://www.cantook.com/api/progression") }
        jsonPath("$.links[2].href") { value(Matchers.containsString("/opds/v2/books/${book.id}/file")) }
        jsonPath("$.readingOrder[0].href") { value(Matchers.containsString("/opds/v2/books/${book.id}/pages/1/raw")) }
        jsonPath("$.readingOrder[0].type") { value("application/pdf") }
        jsonPath("$.resources[0].href") { value(Matchers.containsString("/opds/v2/books/${book.id}/thumbnail")) }
      }
  }

  @Test
  @WithMockCustomUser
  fun `given invalid convert when getting opds v2 page then bad request is returned`() {
    val book = createReadyPdfBook()

    mockMvc
      .get("/opds/v2/books/${book.id}/pages/1") {
        param("convert", "webp")
      }.andExpect {
        status { isBadRequest() }
        status { reason(Matchers.containsString("Invalid conversion format: webp")) }
      }
  }

  @Test
  @WithMockCustomUser(roles = [])
  fun `given user without page streaming role when getting opds v2 page then forbidden is returned`() {
    val book = createReadyPdfBook()

    mockMvc
      .get("/opds/v2/books/${book.id}/pages/1")
      .andExpect {
        status { isForbidden() }
      }
  }

  @Nested
  inner class LimitedUser {
    @Test
    @WithMockCustomUser(sharedAllLibraries = false, sharedLibraries = ["1"])
    fun `given user with access to a single library when getting series then only gets series from this library`() {
      val createdSeries =
        makeSeries(name = "series", libraryId = library.id).also { series ->
          seriesLifecycle.createSeries(series).let { created ->
            val books = listOf(makeBook("1", libraryId = library.id))
            seriesLifecycle.addBooks(created, books)
          }
        }

      val otherLibrary = makeLibrary("other")
      libraryRepository.insert(otherLibrary)
      makeSeries(name = "otherSeries", libraryId = otherLibrary.id).let { series ->
        seriesLifecycle.createSeries(series).let { created ->
          val otherBooks = listOf(makeBook("2", libraryId = otherLibrary.id))
          seriesLifecycle.addBooks(created, otherBooks)
        }
      }

      mockMvc
        .get("/opds/v1.2/series")
        .andExpect {
          status { isOk() }
          xpath("/feed/entry/id") {
            nodeCount(1)
            string(createdSeries.id)
          }
        }
    }
  }

  @Nested
  inner class ContentRestriction {
    @Test
    @WithMockCustomUser(allowAgeUnder = 10)
    fun `given user only allowed content with specific age rating when getting series then only gets series that satisfies this criteria`() {
      val series10 =
        makeSeries(name = "series_10", libraryId = library.id).also { series ->
          seriesLifecycle.createSeries(series).also { created ->
            val books = listOf(makeBook("1", libraryId = library.id))
            seriesLifecycle.addBooks(created, books)
          }
          seriesMetadataRepository.findById(series.id).let {
            seriesMetadataRepository.update(it.copy(ageRating = 10))
          }
        }

      val series5 =
        makeSeries(name = "series_5", libraryId = library.id).also { series ->
          seriesLifecycle.createSeries(series).also { created ->
            val books = listOf(makeBook("1", libraryId = library.id))
            seriesLifecycle.addBooks(created, books)
          }
          seriesMetadataRepository.findById(series.id).let {
            seriesMetadataRepository.update(it.copy(ageRating = 5))
          }
        }

      val series15 =
        makeSeries(name = "series_15", libraryId = library.id).also { series ->
          seriesLifecycle.createSeries(series).also { created ->
            val books = listOf(makeBook("1", libraryId = library.id))
            seriesLifecycle.addBooks(created, books)
          }
          seriesMetadataRepository.findById(series.id).let {
            seriesMetadataRepository.update(it.copy(ageRating = 15))
          }
        }

      val series =
        makeSeries(name = "series_no", libraryId = library.id).also { series ->
          seriesLifecycle.createSeries(series).also { created ->
            val books = listOf(makeBook("1", libraryId = library.id))
            seriesLifecycle.addBooks(created, books)
          }
        }

      mockMvc.get("/opds/v1.2/series/${series5.id}").andExpect { status { isOk() } }
      mockMvc.get("/opds/v1.2/series/${series10.id}").andExpect { status { isOk() } }
      mockMvc.get("/opds/v1.2/series/${series15.id}").andExpect { status { isForbidden() } }
      mockMvc.get("/opds/v1.2/series/${series.id}").andExpect { status { isForbidden() } }

      mockMvc
        .get("/opds/v1.2/series")
        .andExpect {
          status { isOk() }
          xpath("/feed/entry/id") { nodeCount(2) }
          xpath("/feed/entry[1]/id") { string(series10.id) }
          xpath("/feed/entry[2]/id") { string(series5.id) }
        }
    }

    @Test
    @WithMockCustomUser(excludeAgeOver = 16)
    fun `given user disallowed content with specific age rating when getting series then only gets series that satisfies this criteria`() {
      val series10 =
        makeSeries(name = "series_10", libraryId = library.id).also { series ->
          seriesLifecycle.createSeries(series).also { created ->
            val books = listOf(makeBook("1", libraryId = library.id))
            seriesLifecycle.addBooks(created, books)
          }
          seriesMetadataRepository.findById(series.id).let {
            seriesMetadataRepository.update(it.copy(ageRating = 10))
          }
        }

      val series18 =
        makeSeries(name = "series_18", libraryId = library.id).also { series ->
          seriesLifecycle.createSeries(series).also { created ->
            val books = listOf(makeBook("1", libraryId = library.id))
            seriesLifecycle.addBooks(created, books)
          }
          seriesMetadataRepository.findById(series.id).let {
            seriesMetadataRepository.update(it.copy(ageRating = 18))
          }
        }

      val series16 =
        makeSeries(name = "series_16", libraryId = library.id).also { series ->
          seriesLifecycle.createSeries(series).also { created ->
            val books = listOf(makeBook("1", libraryId = library.id))
            seriesLifecycle.addBooks(created, books)
          }
          seriesMetadataRepository.findById(series.id).let {
            seriesMetadataRepository.update(it.copy(ageRating = 16))
          }
        }

      val series =
        makeSeries(name = "series_no", libraryId = library.id).also { series ->
          seriesLifecycle.createSeries(series).also { created ->
            val books = listOf(makeBook("1", libraryId = library.id))
            seriesLifecycle.addBooks(created, books)
          }
        }

      mockMvc.get("/opds/v1.2/series/${series.id}").andExpect { status { isOk() } }
      mockMvc.get("/opds/v1.2/series/${series10.id}").andExpect { status { isOk() } }
      mockMvc.get("/opds/v1.2/series/${series16.id}").andExpect { status { isForbidden() } }
      mockMvc.get("/opds/v1.2/series/${series18.id}").andExpect { status { isForbidden() } }

      mockMvc
        .get("/opds/v1.2/series")
        .andExpect {
          status { isOk() }
          xpath("/feed/entry/id") { nodeCount(2) }
          xpath("/feed/entry[1]/id") { string(series10.id) }
          xpath("/feed/entry[2]/id") { string(series.id) }
        }
    }
  }

  @Nested
  inner class SeriesSort {
    @Test
    @WithMockCustomUser
    fun `given series with titleSort when requesting via opds then series are sorted by titleSort`() {
      val alphaC = seriesLifecycle.createSeries(makeSeries("TheAlpha", libraryId = library.id))
      seriesMetadataRepository.findById(alphaC.id).let {
        seriesMetadataRepository.update(it.copy(titleSort = "Alpha, The"))
      }
      seriesLifecycle.createSeries(makeSeries("Beta", libraryId = library.id))

      mockMvc
        .get("/opds/v1.2/series")
        .andExpect {
          status { isOk() }
          xpath("/feed/entry[1]/title") { string("TheAlpha") }
          xpath("/feed/entry[2]/title") { string("Beta") }
        }
    }

    @Test
    @WithMockCustomUser
    fun `given series when requesting via opds then series are sorted insensitive of case`() {
      listOf("a", "b", "B", "C")
        .map { name -> makeSeries(name, libraryId = library.id) }
        .forEach {
          seriesLifecycle.createSeries(it)
        }

      mockMvc
        .get("/opds/v1.2/series")
        .andExpect {
          status { isOk() }
          xpath("/feed/entry[1]/title") { string("a") }
          xpath("/feed/entry[2]/title") { string(Matchers.equalToIgnoringCase("b")) }
          xpath("/feed/entry[3]/title") { string(Matchers.equalToIgnoringCase("b")) }
          xpath("/feed/entry[4]/title") { string("C") }
        }
    }
  }

  @Nested
  inner class SeriesStatus {
    @Test
    @WithMockCustomUser
    fun `given series when requesting via opds then deleted series are not returned`() {
      seriesLifecycle.createSeries(makeSeries("Alpha", libraryId = library.id)).also {
        seriesRepository.update(it.copy(deletedDate = LocalDateTime.now()))
      }
      seriesLifecycle.createSeries(makeSeries("Beta", libraryId = library.id))

      mockMvc
        .get("/opds/v1.2/series")
        .andExpect {
          status { isOk() }
          xpath("/feed/entry") { nodeCount(1) }
          xpath("/feed/entry[1]/title") { string("Beta") }
        }
    }
  }

  @Nested
  inner class BookOrdering {
    @Test
    @WithMockCustomUser
    fun `given books with unordered index when requesting via opds then books are ordered`() {
      val createdSeries =
        makeSeries(name = "series", libraryId = library.id).let { series ->
          seriesLifecycle.createSeries(series).also { created ->
            val books = listOf(makeBook("1", libraryId = library.id), makeBook("3", libraryId = library.id))
            seriesLifecycle.addBooks(created, books)
          }
        }

      val addedBook = makeBook("2", libraryId = library.id)
      seriesLifecycle.addBooks(createdSeries, listOf(addedBook))
      seriesLifecycle.sortBooks(createdSeries)

      bookRepository.findAll().forEach {
        mediaRepository.findById(it.id).let { media ->
          mediaRepository.update(media.copy(status = Media.Status.READY, pages = listOf(BookPage("1.jpg", "image/jpeg"))))
        }
      }

      mockMvc
        .get("/opds/v1.2/series/${createdSeries.id}")
        .andExpect {
          status { isOk() }
          xpath("/feed/entry[1]/title") { string("1") }
          xpath("/feed/entry[2]/title") { string("2") }
          xpath("/feed/entry[3]/title") { string("3") }
        }
    }
  }

  @Nested
  inner class BookStatus {
    @Test
    @WithMockCustomUser
    fun `given books not ready when requesting via opds then no books are returned`() {
      val createdSeries =
        makeSeries(name = "series", libraryId = library.id).let { series ->
          seriesLifecycle.createSeries(series).also { created ->
            val books = listOf(makeBook("1", libraryId = library.id), makeBook("3", libraryId = library.id))
            seriesLifecycle.addBooks(created, books)
          }
        }

      bookRepository.findAll().forEach {
        mediaRepository.findById(it.id).let { media ->
          mediaRepository.update(media.copy(status = Media.Status.READY, pages = listOf(BookPage("1.jpg", "image/jpeg"))))
        }
      }

      val addedBook = makeBook("2", libraryId = library.id)
      seriesLifecycle.addBooks(createdSeries, listOf(addedBook))
      seriesLifecycle.sortBooks(createdSeries)

      mockMvc
        .get("/opds/v1.2/series/${createdSeries.id}")
        .andExpect {
          status { isOk() }
          xpath("/feed/entry") { nodeCount(2) }
          xpath("/feed/entry[1]/title") { string("1") }
          xpath("/feed/entry[2]/title") { string("3") }
        }
    }

    @Test
    @WithMockCustomUser
    fun `given deleted ready books when requesting via opds then no books are returned`() {
      val createdSeries =
        makeSeries(name = "series", libraryId = library.id).let { series ->
          seriesLifecycle.createSeries(series).also { created ->
            val books = listOf(makeBook("1", libraryId = library.id), makeBook("3", libraryId = library.id))
            seriesLifecycle.addBooks(created, books)
          }
        }

      val addedBook = makeBook("2", libraryId = library.id)
      seriesLifecycle.addBooks(createdSeries, listOf(addedBook))
      seriesLifecycle.sortBooks(createdSeries)

      bookRepository.findAll().forEach {
        mediaRepository.findById(it.id).let { media ->
          mediaRepository.update(media.copy(status = Media.Status.READY, pages = listOf(BookPage("1.jpg", "image/jpeg"))))
        }
        if (it.id == addedBook.id)
          bookRepository.update(it.copy(deletedDate = LocalDateTime.now()))
      }

      mockMvc
        .get("/opds/v1.2/series/${createdSeries.id}")
        .andExpect {
          status { isOk() }
          xpath("/feed/entry") { nodeCount(2) }
          xpath("/feed/entry[1]/title") { string("1") }
          xpath("/feed/entry[2]/title") { string("3") }
        }
    }
  }
}
