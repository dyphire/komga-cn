package org.gotson.komga.interfaces.sse

import com.fasterxml.jackson.databind.ObjectMapper
import io.mockk.mockk
import org.assertj.core.api.Assertions.assertThat
import org.gotson.komga.application.tasks.TasksRepository
import org.gotson.komga.domain.model.DomainEvent
import org.gotson.komga.domain.model.KomgaUser
import org.gotson.komga.domain.model.ReadProgress
import org.gotson.komga.domain.model.UserRoles
import org.gotson.komga.domain.model.makeBook
import org.gotson.komga.domain.model.makeLibrary
import org.gotson.komga.domain.persistence.BookRepository
import org.junit.jupiter.api.Test

class SseControllerTest {
  private val bookRepository = mockk<BookRepository>(relaxed = true)
  private val tasksRepository = mockk<TasksRepository>(relaxed = true)
  private val controller = SseController(bookRepository, tasksRepository)
  private val objectMapper = ObjectMapper().findAndRegisterModules()

  @Test
  fun `given book updated event when described then snapshot matches`() {
    val book = makeBook(name = "Book 1", id = "book-1", seriesId = "series-1", libraryId = "library-1")

    val actual = controller.describeEvent(DomainEvent.BookUpdated(book))

    assertThat(actual).isNotNull
    assertSseSnapshot(objectMapper, "book-changed.json", actual!!)
  }

  @Test
  fun `given read progress changed event when described then snapshot matches`() {
    val progress = ReadProgress(bookId = "book-1", userId = "user-1", page = 3, completed = false)

    val actual = controller.describeEvent(DomainEvent.ReadProgressChanged(progress))

    assertThat(actual).isNotNull
    assertSseSnapshot(objectMapper, "read-progress-changed.json", actual!!)
  }

  @Test
  fun `given session expired event when described then snapshot matches`() {
    val user = KomgaUser(email = "user@example.org", password = "password", id = "user-1")

    val actual = controller.describeEvent(DomainEvent.UserUpdated(user, expireSession = true))

    assertThat(actual).isNotNull
    assertSseSnapshot(objectMapper, "session-expired.json", actual!!)
  }

  @Test
  fun `given task counts when described then snapshot matches`() {
    val actual = controller.describeTaskQueueStatus(mapOf("scanLibrary" to 2, "analyzeBook" to 1))

    assertSseSnapshot(objectMapper, "task-queue-status.json", actual)
  }

  @Test
  fun `given library scanned event when described then no sse event is emitted`() {
    val actual = controller.describeEvent(DomainEvent.LibraryScanned(makeLibrary(id = "library-1")))

    assertThat(actual).isNull()
  }

  @Test
  fun `given read progress changed event then it is user scoped`() {
    val progress = ReadProgress(bookId = "book-1", userId = "user-1", page = 3, completed = false)

    val actual = controller.describeEvent(DomainEvent.ReadProgressChanged(progress))

    assertThat(actual?.userIdOnly).isEqualTo("user-1")
    assertThat(actual?.adminOnly).isFalse()
  }

  @Test
  fun `given task queue status event then it is admin scoped`() {
    val actual = controller.describeTaskQueueStatus(mapOf("scanLibrary" to 2))

    assertThat(actual.adminOnly).isTrue()
    assertThat(actual.userIdOnly).isNull()
  }
}
