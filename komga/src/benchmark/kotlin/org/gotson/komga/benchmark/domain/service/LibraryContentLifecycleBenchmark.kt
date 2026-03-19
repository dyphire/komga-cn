package org.gotson.komga.benchmark.domain.service

import org.gotson.komga.benchmark.AbstractBenchmark
import org.gotson.komga.benchmark.rest.BenchmarkDataSeeder
import org.gotson.komga.domain.model.Library
import org.gotson.komga.domain.service.LibraryContentLifecycle
import org.openjdk.jmh.annotations.Benchmark
import org.openjdk.jmh.annotations.Level
import org.openjdk.jmh.annotations.OutputTimeUnit
import org.openjdk.jmh.annotations.Setup
import org.springframework.beans.factory.annotation.Autowired
import java.util.concurrent.TimeUnit

@OutputTimeUnit(TimeUnit.MILLISECONDS)
class LibraryContentLifecycleBenchmark : AbstractBenchmark() {
  companion object {
    private lateinit var libraryContentLifecycle: LibraryContentLifecycle
    private lateinit var benchmarkDataSeeder: BenchmarkDataSeeder
  }

  @Autowired
  fun setLibraryContentLifecycle(libraryContentLifecycle: LibraryContentLifecycle) {
    Companion.libraryContentLifecycle = libraryContentLifecycle
  }

  @Autowired
  fun setBenchmarkDataSeeder(benchmarkDataSeeder: BenchmarkDataSeeder) {
    Companion.benchmarkDataSeeder = benchmarkDataSeeder
  }

  private lateinit var library: Library

  @Setup(Level.Trial)
  fun prepareData() {
    library = benchmarkDataSeeder.ensureScanBenchmarkData()
  }

  @Benchmark
  fun scanRootFolder() {
    libraryContentLifecycle.scanRootFolder(library)
  }
}
