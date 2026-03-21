package org.gotson.komga.interfaces.scheduler

import io.github.oshai.kotlinlogging.KotlinLogging
import org.apache.commons.lang3.RandomStringUtils
import org.gotson.komga.domain.model.ApiKey
import org.gotson.komga.domain.model.KomgaUser
import org.gotson.komga.domain.model.UserRoles
import org.gotson.komga.domain.persistence.KomgaUserRepository
import org.gotson.komga.domain.service.KomgaUserLifecycle
import org.gotson.komga.infrastructure.security.TokenEncoder
import org.springframework.boot.context.event.ApplicationReadyEvent
import org.springframework.context.annotation.Bean
import org.springframework.context.annotation.Configuration
import org.springframework.context.annotation.Profile
import org.springframework.context.event.EventListener
import org.springframework.stereotype.Component

private val logger = KotlinLogging.logger {}

private const val COMPAT_API_KEY_USER_EMAIL = "user@example.org"
private const val COMPAT_API_KEY = "compat-api-key"

@Profile("!test & noclaim")
@Component
class InitialUserController(
  private val userLifecycle: KomgaUserLifecycle,
  private val initialUsers: List<KomgaUser>,
  private val userRepository: KomgaUserRepository,
  private val tokenEncoder: TokenEncoder,
) {
  @EventListener(ApplicationReadyEvent::class)
  fun createInitialUserOnStartupIfNoneExist() {
    if (userLifecycle.countUsers() == 0L) {
      logger.info { "No users exist in database, creating initial users" }

      initialUsers
        .forEach {
          val createdUser = userLifecycle.createUser(it)
          seedCompatApiKey(createdUser)
          logger.info { "Initial user created. Login: ${it.email}, Password: ${it.password}" }
        }
    }
  }

  private fun seedCompatApiKey(user: KomgaUser) {
    if (user.email != COMPAT_API_KEY_USER_EMAIL) return

    userRepository.insert(
      ApiKey(
        userId = user.id,
        key = tokenEncoder.encode(COMPAT_API_KEY),
        comment = COMPAT_API_KEY,
      ),
    )
    logger.info { "Deterministic compat API key seeded for user: ${user.email}" }
  }
}

@Configuration
@Profile("dev")
class InitialUsersDevConfiguration {
  @Bean
  fun initialUsers(): List<KomgaUser> =
    listOf(
      KomgaUser("admin@example.org", "admin", roles = UserRoles.entries.toSet()),
      KomgaUser("user@example.org", "user"),
      KomgaUser("limited@example.org", "limited"),
    )
}

@Configuration
@Profile("!dev")
class InitialUsersProdConfiguration {
  @Bean
  fun initialUsers(): List<KomgaUser> =
    listOf(
      KomgaUser("admin@example.org", RandomStringUtils.secure().nextAlphanumeric(12), roles = UserRoles.entries.toSet()),
    )
}
