package org.gotson.komga.benchmark.rest

import org.gotson.komga.benchmark.AbstractBenchmark
import org.gotson.komga.interfaces.api.CommonBookController
import org.gotson.komga.domain.persistence.KomgaUserRepository
import org.gotson.komga.infrastructure.security.KomgaPrincipal
import org.gotson.komga.interfaces.api.rest.BookController
import org.gotson.komga.interfaces.api.rest.SeriesController
import org.openjdk.jmh.annotations.Level
import org.openjdk.jmh.annotations.Setup
import org.springframework.beans.factory.annotation.Autowired
import org.springframework.security.authentication.UsernamePasswordAuthenticationToken
import org.springframework.security.core.context.SecurityContextHolder

internal const val DEFAULT_PAGE_SIZE = 20

abstract class AbstractRestBenchmark : AbstractBenchmark() {
  companion object {
    lateinit var userRepository: KomgaUserRepository
    lateinit var benchmarkDataSeeder: BenchmarkDataSeeder
    lateinit var seriesController: SeriesController
    lateinit var bookController: BookController
    lateinit var commonBookController: CommonBookController
  }

  @Autowired
  fun setUserRepository(repository: KomgaUserRepository) {
    userRepository = repository
  }

  @Autowired
  fun setBenchmarkDataSeeder(seeder: BenchmarkDataSeeder) {
    benchmarkDataSeeder = seeder
  }

  @Autowired
  fun setSeriesController(controller: SeriesController) {
    seriesController = controller
  }

  @Autowired
  fun setBookController(controller: BookController) {
    bookController = controller
  }

  @Autowired
  fun setCommonBookController(controller: CommonBookController) {
    commonBookController = controller
  }

  protected lateinit var principal: KomgaPrincipal

  @Setup(Level.Trial)
  fun prepareData() {
    benchmarkDataSeeder.ensureSeedData()

    principal =
      KomgaPrincipal(
        userRepository.findByEmailIgnoreCaseOrNull("admin@example.org")
          ?: throw IllegalStateException("no user found"),
      )

    SecurityContextHolder.getContext().authentication =
      UsernamePasswordAuthenticationToken(principal, "", principal.authorities)
  }
}
