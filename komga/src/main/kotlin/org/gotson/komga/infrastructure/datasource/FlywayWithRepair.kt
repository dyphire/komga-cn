package org.gotson.komga.infrastructure.datasource

import io.github.oshai.kotlinlogging.KotlinLogging
import org.springframework.boot.autoconfigure.flyway.FlywayMigrationStrategy
import org.springframework.context.annotation.Bean
import org.springframework.context.annotation.Configuration

private val logger = KotlinLogging.logger { }

@Configuration
class FlywayWithRepair {
  @Bean
  fun migrateAndRepair(): FlywayMigrationStrategy {
    // do repair then migrate to handle some problematic stuff
    return FlywayMigrationStrategy { flyway ->
      logger.info { "Repairing and migrating Flyway stuff..." }
      flyway.repair().let {
        logger.info { "Flyway database has been repaired, now migrating..." }
      }
      flyway.migrate()
    }
  }
}
