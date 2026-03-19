package org.gotson.komga.interfaces.sse

import io.github.oshai.kotlinlogging.KotlinLogging
import org.gotson.komga.application.tasks.TasksRepository
import org.gotson.komga.domain.model.DomainEvent
import org.gotson.komga.domain.model.KomgaUser
import org.gotson.komga.domain.persistence.BookRepository
import org.gotson.komga.infrastructure.security.KomgaPrincipal
import org.gotson.komga.infrastructure.web.toFilePath
import org.gotson.komga.interfaces.sse.dto.BookImportSseDto
import org.gotson.komga.interfaces.sse.dto.BookSseDto
import org.gotson.komga.interfaces.sse.dto.CollectionSseDto
import org.gotson.komga.interfaces.sse.dto.LibrarySseDto
import org.gotson.komga.interfaces.sse.dto.ReadListSseDto
import org.gotson.komga.interfaces.sse.dto.ReadProgressSeriesSseDto
import org.gotson.komga.interfaces.sse.dto.ReadProgressSseDto
import org.gotson.komga.interfaces.sse.dto.SeriesSseDto
import org.gotson.komga.interfaces.sse.dto.SessionExpiredDto
import org.gotson.komga.interfaces.sse.dto.TaskQueueSseDto
import org.gotson.komga.interfaces.sse.dto.ThumbnailBookSseDto
import org.gotson.komga.interfaces.sse.dto.ThumbnailReadListSseDto
import org.gotson.komga.interfaces.sse.dto.ThumbnailSeriesCollectionSseDto
import org.gotson.komga.interfaces.sse.dto.ThumbnailSeriesSseDto
import org.springframework.context.SmartLifecycle
import org.springframework.context.event.EventListener
import org.springframework.http.MediaType
import org.springframework.scheduling.annotation.Scheduled
import org.springframework.security.core.annotation.AuthenticationPrincipal
import org.springframework.stereotype.Controller
import org.springframework.web.bind.annotation.GetMapping
import org.springframework.web.servlet.mvc.method.annotation.SseEmitter
import java.io.IOException
import java.util.Collections

private val logger = KotlinLogging.logger {}

data class SseEventDescriptor(
  val name: String,
  val payload: Any,
  val adminOnly: Boolean = false,
  val userIdOnly: String? = null,
)

@Controller
class SseController(
  private val bookRepository: BookRepository,
  private val tasksRepository: TasksRepository,
) : SmartLifecycle {
  private var acceptingConnections = true
  private val emitters = Collections.synchronizedMap(HashMap<SseEmitter, KomgaUser>())

  @GetMapping("sse/v1/events")
  fun sse(
    @AuthenticationPrincipal principal: KomgaPrincipal,
  ): SseEmitter {
    if (!acceptingConnections) throw IllegalStateException("Server is shutting down, not accepting new SSE connections")
    val emitter = SseEmitter()
    emitter.onCompletion { synchronized(emitters) { emitters.remove(emitter) } }
    emitter.onTimeout { synchronized(emitters) { emitters.remove(emitter) } }
    emitter.onError { synchronized(emitters) { emitters.remove(emitter) } }
    emitters[emitter] = principal.user
    return emitter
  }

  @Scheduled(fixedRate = 15_000)
  fun heartbeat() {
    if (emitters.isNotEmpty())
      synchronized(emitters) {
        emitters.forEach { (emitter, _) ->
          try {
            emitter.send(SseEmitter.event().comment("heartbeat"))
          } catch (_: IOException) {
          }
        }
      }
  }

  @Scheduled(fixedRate = 10_000)
  fun taskCount() {
    if (emitters.isNotEmpty()) {
      val tasksCount = tasksRepository.countBySimpleType()
      emitSse(describeTaskQueueStatus(tasksCount))
    }
  }

  internal fun describeTaskQueueStatus(tasksCount: Map<String, Int>): SseEventDescriptor =
    SseEventDescriptor(
      name = "TaskQueueStatus",
      payload = TaskQueueSseDto(tasksCount.values.sum(), tasksCount),
      adminOnly = true,
    )

  @EventListener
  fun handleSseEvent(event: DomainEvent) {
    describeEvent(event)?.let(::emitSse)
  }

  internal fun describeEvent(event: DomainEvent): SseEventDescriptor? =
    when (event) {
      is DomainEvent.LibraryAdded -> SseEventDescriptor("LibraryAdded", LibrarySseDto(event.library.id))
      is DomainEvent.LibraryUpdated -> SseEventDescriptor("LibraryChanged", LibrarySseDto(event.library.id))
      is DomainEvent.LibraryDeleted -> SseEventDescriptor("LibraryDeleted", LibrarySseDto(event.library.id))
      is DomainEvent.LibraryScanned -> null

      is DomainEvent.SeriesAdded -> SseEventDescriptor("SeriesAdded", SeriesSseDto(event.series.id, event.series.libraryId))
      is DomainEvent.SeriesUpdated -> SseEventDescriptor("SeriesChanged", SeriesSseDto(event.series.id, event.series.libraryId))
      is DomainEvent.SeriesDeleted -> SseEventDescriptor("SeriesDeleted", SeriesSseDto(event.series.id, event.series.libraryId))

      is DomainEvent.BookAdded -> SseEventDescriptor("BookAdded", BookSseDto(event.book.id, event.book.seriesId, event.book.libraryId))
      is DomainEvent.BookUpdated -> SseEventDescriptor("BookChanged", BookSseDto(event.book.id, event.book.seriesId, event.book.libraryId))
      is DomainEvent.BookDeleted -> SseEventDescriptor("BookDeleted", BookSseDto(event.book.id, event.book.seriesId, event.book.libraryId))
      is DomainEvent.BookImported -> SseEventDescriptor("BookImported", BookImportSseDto(event.book?.id, event.sourceFile.toFilePath(), event.success, event.message), adminOnly = true)

      is DomainEvent.ReadListAdded -> SseEventDescriptor("ReadListAdded", ReadListSseDto(event.readList.id, event.readList.bookIds.map { it.value }))
      is DomainEvent.ReadListUpdated -> SseEventDescriptor("ReadListChanged", ReadListSseDto(event.readList.id, event.readList.bookIds.map { it.value }))
      is DomainEvent.ReadListDeleted -> SseEventDescriptor("ReadListDeleted", ReadListSseDto(event.readList.id, event.readList.bookIds.map { it.value }))

      is DomainEvent.CollectionAdded -> SseEventDescriptor("CollectionAdded", CollectionSseDto(event.collection.id, event.collection.seriesIds))
      is DomainEvent.CollectionUpdated -> SseEventDescriptor("CollectionChanged", CollectionSseDto(event.collection.id, event.collection.seriesIds))
      is DomainEvent.CollectionDeleted -> SseEventDescriptor("CollectionDeleted", CollectionSseDto(event.collection.id, event.collection.seriesIds))

      is DomainEvent.ReadProgressChanged -> SseEventDescriptor("ReadProgressChanged", ReadProgressSseDto(event.progress.bookId, event.progress.userId), userIdOnly = event.progress.userId)
      is DomainEvent.ReadProgressDeleted -> SseEventDescriptor("ReadProgressDeleted", ReadProgressSseDto(event.progress.bookId, event.progress.userId), userIdOnly = event.progress.userId)
      is DomainEvent.ReadProgressSeriesChanged -> SseEventDescriptor("ReadProgressSeriesChanged", ReadProgressSeriesSseDto(event.seriesId, event.userId), userIdOnly = event.userId)
      is DomainEvent.ReadProgressSeriesDeleted -> SseEventDescriptor("ReadProgressSeriesDeleted", ReadProgressSeriesSseDto(event.seriesId, event.userId), userIdOnly = event.userId)

      is DomainEvent.ThumbnailBookAdded -> SseEventDescriptor("ThumbnailBookAdded", ThumbnailBookSseDto(event.thumbnail.bookId, bookRepository.getSeriesIdOrNull(event.thumbnail.bookId).orEmpty(), event.thumbnail.selected))
      is DomainEvent.ThumbnailBookDeleted -> SseEventDescriptor("ThumbnailBookDeleted", ThumbnailBookSseDto(event.thumbnail.bookId, bookRepository.getSeriesIdOrNull(event.thumbnail.bookId).orEmpty(), event.thumbnail.selected))
      is DomainEvent.ThumbnailSeriesAdded -> SseEventDescriptor("ThumbnailSeriesAdded", ThumbnailSeriesSseDto(event.thumbnail.seriesId, event.thumbnail.selected))
      is DomainEvent.ThumbnailSeriesDeleted -> SseEventDescriptor("ThumbnailSeriesDeleted", ThumbnailSeriesSseDto(event.thumbnail.seriesId, event.thumbnail.selected))
      is DomainEvent.ThumbnailSeriesCollectionAdded -> SseEventDescriptor("ThumbnailSeriesCollectionAdded", ThumbnailSeriesCollectionSseDto(event.thumbnail.collectionId, event.thumbnail.selected))
      is DomainEvent.ThumbnailSeriesCollectionDeleted -> SseEventDescriptor("ThumbnailSeriesCollectionDeleted", ThumbnailSeriesCollectionSseDto(event.thumbnail.collectionId, event.thumbnail.selected))
      is DomainEvent.ThumbnailReadListAdded -> SseEventDescriptor("ThumbnailReadListAdded", ThumbnailReadListSseDto(event.thumbnail.readListId, event.thumbnail.selected))
      is DomainEvent.ThumbnailReadListDeleted -> SseEventDescriptor("ThumbnailReadListDeleted", ThumbnailReadListSseDto(event.thumbnail.readListId, event.thumbnail.selected))

      is DomainEvent.UserUpdated -> if (event.expireSession) SseEventDescriptor("SessionExpired", SessionExpiredDto(event.user.id), userIdOnly = event.user.id) else null
      is DomainEvent.UserDeleted -> SseEventDescriptor("SessionExpired", SessionExpiredDto(event.user.id), userIdOnly = event.user.id)
    }

  private fun emitSse(event: SseEventDescriptor) = emitSse(event.name, event.payload, event.adminOnly, event.userIdOnly)

  private fun emitSse(
    name: String,
    data: Any,
    adminOnly: Boolean = false,
    userIdOnly: String? = null,
  ) {
    logger.debug { "Publish SSE: '$name':$data" }

    synchronized(emitters) {
      emitters
        .filter { if (adminOnly) it.value.isAdmin else true }
        .filter { if (userIdOnly != null) it.value.id == userIdOnly else true }
        .forEach { (emitter, _) ->
          try {
            emitter.send(
              SseEmitter
                .event()
                .name(name)
                .data(data, MediaType.APPLICATION_JSON),
            )
          } catch (_: IOException) {
          }
        }
    }
  }

  override fun start() = Unit

  override fun stop() {
    logger.debug { "Closing all SSE connections" }
    acceptingConnections = false
    synchronized(emitters) {
      emitters.forEach { (emitter, _) -> emitter.complete() }
    }
  }

  override fun isRunning(): Boolean = true

  override fun getPhase(): Int = SmartLifecycle.DEFAULT_PHASE
}
