package org.gotson.komga.infrastructure.configuration

import jakarta.annotation.PostConstruct
import jakarta.validation.constraints.NotBlank
import jakarta.validation.constraints.Positive
import org.springframework.boot.context.properties.ConfigurationProperties
import org.springframework.boot.convert.DurationUnit
import org.springframework.stereotype.Component
import org.springframework.validation.annotation.Validated
import org.sqlite.SQLiteConfig.JournalMode
import java.time.Duration
import java.time.temporal.ChronoUnit
import kotlin.io.path.Path
import kotlin.io.path.createDirectories

@Component
@ConfigurationProperties(prefix = "komga")
@Validated
class KomgaProperties {
  @PostConstruct
  private fun makeDirs() {
    try {
      Path(database.file).parent.createDirectories()
      Path(tasksDb.file).parent.createDirectories()
    } catch (_: Exception) {
    }
  }

  var findDuplicatePages: Boolean = true

  @Positive
  var pageHashing: Int = 3

  @Positive
  var epubDivinaLetterCountThreshold: Int = 15

  var oauth2AccountCreation: Boolean = false

  var oidcEmailVerification: Boolean = true

  var database = Database()

  var tasksDb = Database()

  var cors = Cors()

  var lucene = Lucene()

  var configDir: String? = null

  var kobo = Kobo()

  var analysis = Analysis()

  val fonts = Fonts()

  class Cors {
    var allowedOrigins: List<String> = emptyList()
  }

  class Database {
    @get:NotBlank
    var file: String = ""

    @get:Positive
    var batchChunkSize: Int = 1000

    @get:Positive
    var poolSize: Int? = null

    @get:Positive
    var maxPoolSize: Int = 1

    var journalMode: JournalMode? = JournalMode.WAL

    @DurationUnit(ChronoUnit.SECONDS)
    var busyTimeout: Duration? = null

    var pragmas: Map<String, String> = emptyMap()

    var checkLocalFilesystem: Boolean = true
  }

  class Fonts {
    @get:NotBlank
    var dataDirectory: String = ""
  }

  class Lucene {
    @get:NotBlank
    var dataDirectory: String = ""

    var indexAnalyzer = IndexAnalyzer()

    @DurationUnit(ChronoUnit.SECONDS)
    var commitDelay: Duration = Duration.ofSeconds(2)

    class IndexAnalyzer {
      @get:Positive
      var minGram: Int = 3

      @get:Positive
      var maxGram: Int = 10

      var preserveOriginal: Boolean = true
    }
  }

  class Kobo {
    @get:Positive
    var syncItemLimit: Int = 100

    var kepubifyPath: String? = null
  }

  class Analysis {
    @get:Positive
    var adPagesCheckCount: Int = 10

    @get:Positive
    var maxImageSizeForAnalysis: Long = 10 * 1024 * 1024 // 10MB

    @get:Positive
    var concurrentAnalysisThreads: Int = Runtime.getRuntime().availableProcessors()

    var enableAnalysisCache: Boolean = true

    @get:Positive
    var mediaTypeCacheSize: Int = 1000

    @DurationUnit(ChronoUnit.HOURS)
    var mediaTypeCacheExpireHours: Duration = Duration.ofHours(1)

    @get:Positive
    var imageAnalysisCacheSize: Int = 500

    @DurationUnit(ChronoUnit.MINUTES)
    var imageAnalysisCacheExpireMinutes: Duration = Duration.ofMinutes(30)

    // Circuit breaker configuration
    var circuitBreakerEnabled: Boolean = true

    var memoryThreshold: Double = 0.8 // 80% memory usage threshold

    var cpuThreshold: Double = 0.8 // 80% CPU usage threshold

    var dbConnectionThreshold: Double = 0.8 // 80% DB connection pool usage

    var circuitBreakerCooldownMs: Duration = Duration.ofMillis(30000) // 30 seconds cooldown
  }
}
