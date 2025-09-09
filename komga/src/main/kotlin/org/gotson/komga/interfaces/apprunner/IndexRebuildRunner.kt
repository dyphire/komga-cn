package org.gotson.komga.interfaces.apprunner

import io.github.oshai.kotlinlogging.KotlinLogging
import org.gotson.komga.application.tasks.TaskEmitter
import org.gotson.komga.infrastructure.search.LuceneHelper
import org.springframework.boot.ApplicationArguments
import org.springframework.boot.ApplicationRunner
import org.springframework.context.annotation.Profile
import org.springframework.stereotype.Component

private val logger = KotlinLogging.logger {}

@Profile("!test")
@Component
class IndexRebuildRunner(
  private val luceneHelper: LuceneHelper,
  private val taskEmitter: TaskEmitter,
) : ApplicationRunner {

  override fun run(args: ApplicationArguments) {
    logger.info { "Check the search index status..." }

    val forceRebuild = args.getOptionValues("rebuild-index") != null

    if (forceRebuild || !luceneHelper.indexExists()) {
      logger.info { "Submitting search index rebuild task..." }
      try {
        taskEmitter.rebuildIndex()
        logger.info { "Search index rebuild task submitted" }
      } catch (e: Exception) {
        logger.error(e) { "Error occurred while submitting search index rebuild task" }
        throw e
      }
    } else {
      logger.info { "Search index already exists, skipping rebuild" }
    }
  }
}
