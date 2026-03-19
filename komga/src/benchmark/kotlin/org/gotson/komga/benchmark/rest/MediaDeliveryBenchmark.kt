package org.gotson.komga.benchmark.rest

import org.gotson.komga.domain.model.BookSearch
import org.gotson.komga.domain.model.SearchCondition
import org.gotson.komga.domain.model.SearchOperator
import org.gotson.komga.domain.model.SeriesSearch
import org.openjdk.jmh.annotations.Benchmark
import org.openjdk.jmh.annotations.Level
import org.openjdk.jmh.annotations.OutputTimeUnit
import org.openjdk.jmh.annotations.Setup
import org.springframework.data.domain.PageRequest
import org.springframework.data.domain.Sort
import org.springframework.http.HttpHeaders
import org.springframework.mock.web.MockHttpServletRequest
import org.springframework.web.context.request.ServletWebRequest
import java.io.OutputStream
import java.time.ZoneOffset
import java.time.ZonedDateTime
import java.time.format.DateTimeFormatter
import java.util.concurrent.TimeUnit

@OutputTimeUnit(TimeUnit.MILLISECONDS)
class MediaDeliveryBenchmark : AbstractRestBenchmark() {
  companion object {
    private lateinit var bookId: String
    private const val pageNumber = 1
    private val futureIfModifiedSince =
      ZonedDateTime.of(2099, 1, 1, 0, 0, 0, 0, ZoneOffset.UTC).format(DateTimeFormatter.RFC_1123_DATE_TIME)
  }

  @Setup(Level.Trial)
  override fun prepareData() {
    super.prepareData()

    val biggestSeriesId =
      seriesController
        .getSeries(principal, page = PageRequest.of(0, 1, Sort.by(Sort.Order.desc("booksCount"))), search = SeriesSearch())
        .content
        .first()
        .id

    bookId =
      bookController
        .getBooks(
          principal,
          page = PageRequest.of(0, 1, Sort.by(Sort.Order.asc("metadata.numberSort"))),
          search = BookSearch(SearchCondition.SeriesId(SearchOperator.Is(biggestSeriesId))),
        ).content
        .first()
        .id
  }

  @Benchmark
  fun pageReadCacheHit304() {
    bookController.getBookPageByNumber(
      principal = principal,
      request = ServletWebRequest(MockHttpServletRequest().apply { addHeader(HttpHeaders.IF_MODIFIED_SINCE, futureIfModifiedSince) }),
      bookId = bookId,
      pageNumber = pageNumber,
      convertTo = null,
      zeroBasedIndex = false,
      acceptHeaders = null,
      contentNegotiation = true,
    )
  }

  @Benchmark
  fun pageRead() {
    bookController.getBookPageByNumber(
      principal = principal,
      request = ServletWebRequest(MockHttpServletRequest()),
      bookId = bookId,
      pageNumber = pageNumber,
      convertTo = null,
      zeroBasedIndex = false,
      acceptHeaders = null,
      contentNegotiation = true,
    )
  }

  @Benchmark
  fun pageThumbnail() {
    bookController.getBookPageThumbnailByNumber(
      principal = principal,
      request = ServletWebRequest(MockHttpServletRequest()),
      bookId = bookId,
      pageNumber = pageNumber,
    )
  }

  @Benchmark
  fun fileDownload() {
    commonBookController.getBookFileInternal(principal, bookId).body?.writeTo(OutputStream.nullOutputStream())
  }
}
