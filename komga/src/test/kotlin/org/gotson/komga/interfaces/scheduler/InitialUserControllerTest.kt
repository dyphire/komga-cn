package org.gotson.komga.interfaces.scheduler

import io.mockk.every
import io.mockk.mockk
import io.mockk.slot
import io.mockk.verify
import org.assertj.core.api.Assertions.assertThat
import org.gotson.komga.domain.model.ApiKey
import org.gotson.komga.domain.model.KomgaUser
import org.gotson.komga.domain.persistence.KomgaUserRepository
import org.gotson.komga.domain.service.KomgaUserLifecycle
import org.gotson.komga.infrastructure.security.TokenEncoder
import org.junit.jupiter.api.Test

class InitialUserControllerTest {
  @Test
  fun `given dev initial users when listing them then limited user exists alongside admin and user`() {
    val users = InitialUsersDevConfiguration().initialUsers()

    assertThat(users)
      .extracting<String> { it.email }
      .containsExactly(
        "admin@example.org",
        "user@example.org",
        "limited@example.org",
      )

    assertThat(users.associateBy { it.email })
      .containsKeys("admin@example.org", "user@example.org", "limited@example.org")

    assertThat(users.single { it.email == "admin@example.org" }.password).isEqualTo("admin")
    assertThat(users.single { it.email == "user@example.org" }.password).isEqualTo("user")
    assertThat(users.single { it.email == "limited@example.org" }.password).isEqualTo("limited")
  }

  @Test
  fun `given no existing users when creating initial users then compat api key is seeded for user bootstrap account`() {
    val userLifecycle = mockk<KomgaUserLifecycle>()
    val userRepository = mockk<KomgaUserRepository>(relaxed = true)
    val tokenEncoder = mockk<TokenEncoder>()
    val initialUsers = InitialUsersDevConfiguration().initialUsers()
    val createdUser = initialUsers.single { it.email == "user@example.org" }.copy(id = "compat-user-id")
    val apiKeySlot = slot<ApiKey>()
    val controller = InitialUserController(userLifecycle, initialUsers, userRepository, tokenEncoder)

    every { userLifecycle.countUsers() } returns 0
    every { userLifecycle.createUser(any()) } answers { firstArg<KomgaUser>().takeIf { it.email != createdUser.email } ?: createdUser }
    every { tokenEncoder.encode("compat-api-key") } returns "encoded-compat-api-key"
    every { userRepository.insert(capture(apiKeySlot)) } returns Unit

    controller.createInitialUserOnStartupIfNoneExist()

    verify(exactly = 3) { userLifecycle.createUser(any()) }
    verify(exactly = 1) { tokenEncoder.encode("compat-api-key") }
    verify(exactly = 1) { userRepository.insert(any<ApiKey>()) }
    assertThat(apiKeySlot.captured.userId).isEqualTo("compat-user-id")
    assertThat(apiKeySlot.captured.key).isEqualTo("encoded-compat-api-key")
    assertThat(apiKeySlot.captured.comment).isEqualTo("compat-api-key")
  }
}
