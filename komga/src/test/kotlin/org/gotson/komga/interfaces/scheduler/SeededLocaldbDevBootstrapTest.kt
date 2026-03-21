package org.gotson.komga.interfaces.scheduler

import org.assertj.core.api.Assertions.assertThat
import org.hamcrest.Matchers.containsString
import org.junit.jupiter.api.Test
import org.springframework.beans.factory.annotation.Autowired
import org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc
import org.springframework.boot.test.context.SpringBootTest
import org.springframework.http.HttpHeaders
import org.springframework.http.MediaType
import org.springframework.security.test.web.servlet.request.SecurityMockMvcRequestPostProcessors.httpBasic
import org.springframework.test.context.ActiveProfiles
import org.springframework.test.web.servlet.MockMvc
import org.springframework.test.web.servlet.get
import java.time.Duration

@SpringBootTest(
  properties = [
    "komga.config-dir=build/tmp/dev-seeded-localdb-bootstrap-test",
    "komga.database.file=file:dev-seeded-localdb-bootstrap-test?mode=memory",
    "komga.tasks-db.file=file:dev-seeded-localdb-bootstrap-tasks?mode=memory",
  ],
)
@AutoConfigureMockMvc(printOnlyOnFailure = false)
@ActiveProfiles("dev", "noclaim")
class SeededLocaldbDevBootstrapTest(
  @Autowired private val mockMvc: MockMvc,
  @Autowired private val sessionHeaderName: String,
) {
  @Test
  fun `given dev noclaim bootstrap when requesting seeded localdb compat surface then compat auth and seeded routes are available`() {
    eventually {
      val adminSessionToken = sessionTokenOnce("admin@example.org", "admin")

      mockMvc
        .get("/api/v1/libraries") {
          header(sessionHeaderName, adminSessionToken)
        }.andExpect {
          status { isOk() }
          content { contentTypeCompatibleWith(MediaType.APPLICATION_JSON) }
          jsonPath("$[0].id") { value("1") }
        }

      mockMvc
        .get("/api/v1/books/book-1/pages/1") {
          header(sessionHeaderName, adminSessionToken)
        }.andExpect {
          status { isOk() }
        }

      mockMvc
        .get("/api/v1/books/book-1/file") {
          header(sessionHeaderName, adminSessionToken)
        }.andExpect {
          status { isOk() }
          header { string(HttpHeaders.CONTENT_TYPE, containsString("application/zip")) }
        }

      mockMvc
        .get("/api/v1/books/book-1/pages/1/thumbnail") {
          header(sessionHeaderName, adminSessionToken)
        }.andExpect {
          status { isOk() }
          header { string(HttpHeaders.CONTENT_TYPE, containsString(MediaType.IMAGE_JPEG_VALUE)) }
        }

      mockMvc
        .get("/opds/v2/books/book-1/manifest") {
          header(sessionHeaderName, adminSessionToken)
        }.andExpect {
          status { isOk() }
          header { string(HttpHeaders.CONTENT_TYPE, containsString("application/opds-publication+json")) }
          jsonPath("$.metadata.title") { value("book.cbr") }
          jsonPath("$.metadata.belongsTo.series[0].name") { value("series") }
        }

      mockMvc
        .get("/api/v2/users/me") {
          header("X-API-Key", "compat-api-key")
        }.andExpect {
          status { isOk() }
          jsonPath("$.email") { value("user@example.org") }
        }
    }
  }

  private fun sessionTokenOnce(
    email: String,
    password: String,
  ): String {
    val response =
      mockMvc
        .get("/api/v2/users/me") {
          with(httpBasic(email, password))
          header(sessionHeaderName, "")
        }.andExpect {
          status { isOk() }
        }.andReturn()
        .response

    return response.getHeader(sessionHeaderName).also {
      assertThat(it).isNotBlank()
    }!!
  }

  private fun eventually(timeout: Duration = Duration.ofSeconds(5), block: () -> Unit) {
    val deadline = System.nanoTime() + timeout.toNanos()
    var lastError: Throwable? = null

    while (System.nanoTime() < deadline) {
      try {
        block()
        return
      } catch (t: Throwable) {
        lastError = t
        Thread.sleep(100)
      }
    }

    throw lastError ?: AssertionError("bootstrap did not complete within $timeout")
  }
}
