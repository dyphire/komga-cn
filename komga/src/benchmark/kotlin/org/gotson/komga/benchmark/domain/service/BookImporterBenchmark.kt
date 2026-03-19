package org.gotson.komga.benchmark.domain.service

import org.gotson.komga.benchmark.AbstractBenchmark
import org.gotson.komga.benchmark.rest.BenchmarkDataSeeder
import org.gotson.komga.domain.model.CopyMode
import org.gotson.komga.domain.model.Series
import org.gotson.komga.domain.persistence.BookRepository
import org.gotson.komga.domain.service.BookImporter
import org.gotson.komga.domain.service.BookLifecycle
import org.openjdk.jmh.annotations.Benchmark
import org.openjdk.jmh.annotations.Level
import org.openjdk.jmh.annotations.OutputTimeUnit
import org.openjdk.jmh.annotations.Setup
import org.springframework.beans.factory.annotation.Autowired
import java.nio.file.Path
import java.util.concurrent.TimeUnit
import kotlin.io.path.deleteIfExists

@OutputTimeUnit(TimeUnit.MILLISECONDS)
class BookImporterBenchmark : AbstractBenchmark() {
  private companion object {
    private const val destinationName = "benchmark-import"
    private lateinit var bookImporter: BookImporter
    private lateinit var bookLifecycle: BookLifecycle
    private lateinit var bookRepository: BookRepository
    private lateinit var benchmarkDataSeeder: BenchmarkDataSeeder
  }

  @Autowired
  fun setBookImporter(bookImporter: BookImporter) {
    Companion.bookImporter = bookImporter
  }

  @Autowired
  fun setBookLifecycle(bookLifecycle: BookLifecycle) {
    Companion.bookLifecycle = bookLifecycle
  }

  @Autowired
  fun setBookRepository(bookRepository: BookRepository) {
    Companion.bookRepository = bookRepository
  }

  @Autowired
  fun setBenchmarkDataSeeder(benchmarkDataSeeder: BenchmarkDataSeeder) {
    Companion.benchmarkDataSeeder = benchmarkDataSeeder
  }

  private lateinit var sourceFile: Path
  private lateinit var series: Series

  @Setup(Level.Trial)
  fun prepareData() {
    val fixture = benchmarkDataSeeder.ensureImportBenchmarkData()
    sourceFile = fixture.first
    series = fixture.second

    val existingBooks = bookRepository.findAllBySeriesId(series.id)
    if (existingBooks.isNotEmpty()) {
      bookLifecycle.deleteMany(existingBooks)
    }

    series.path.resolve("$destinationName.cbz").deleteIfExists()
  }

  @Benchmark
  fun importBook() {
    bookImporter.importBook(sourceFile, series, CopyMode.COPY, destinationName = destinationName)
  }
}
