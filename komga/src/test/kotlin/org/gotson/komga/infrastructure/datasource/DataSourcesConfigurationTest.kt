package org.gotson.komga.infrastructure.datasource

import com.zaxxer.hikari.HikariDataSource
import org.assertj.core.api.Assertions.assertThat
import org.junit.jupiter.api.Nested
import org.junit.jupiter.api.Test
import org.springframework.beans.factory.annotation.Autowired
import org.springframework.beans.factory.annotation.Qualifier
import org.springframework.boot.test.context.SpringBootTest
import org.springframework.test.context.ActiveProfiles
import javax.sql.DataSource

class DataSourcesConfigurationTest {
  @SpringBootTest
  @Nested
  inner class WalMode(
    @Autowired private val dataSourceRW: DataSource,
    @Autowired @Qualifier("sqliteDataSourceRO") private val dataSourceRO: DataSource,
    @Autowired @Qualifier("tasksDataSourceRW") private val tasksDataSourceRW: DataSource,
    @Autowired @Qualifier("tasksDataSourceRO") private val tasksDataSourceRO: DataSource,
  ) {
    private fun queryString(dataSource: DataSource, sql: String): String? =
      dataSource.connection.use { connection ->
        connection.prepareStatement(sql).use { statement ->
          statement.executeQuery().use { resultSet ->
            if (resultSet.next()) resultSet.getString(1) else null
          }
        }
      }

    private fun queryInt(dataSource: DataSource, sql: String): Int =
      dataSource.connection.use { connection ->
        connection.prepareStatement(sql).use { statement ->
          statement.executeQuery().use { resultSet ->
            resultSet.next()
            resultSet.getInt(1)
          }
        }
      }

    @Test
    fun `given wal mode when autoriwiring beans then bean instances are different between RW and RO`() {
      assertThat(dataSourceRW).isNotSameAs(dataSourceRO)
      assertThat(tasksDataSourceRW).isNotSameAs(tasksDataSourceRO)
    }

    @Test
    fun `given wal mode when inspecting datasources then write pools are single writer and migrations are applied`() {
      assertThat((dataSourceRW as HikariDataSource).maximumPoolSize).isEqualTo(1)
      assertThat((tasksDataSourceRW as HikariDataSource).maximumPoolSize).isEqualTo(1)

      assertThat(queryString(dataSourceRW, "PRAGMA journal_mode")).isEqualToIgnoringCase("wal")
      assertThat(queryString(dataSourceRO, "PRAGMA journal_mode")).isEqualToIgnoringCase("wal")
      assertThat(queryString(tasksDataSourceRW, "PRAGMA journal_mode")).isEqualToIgnoringCase("wal")
      assertThat(queryString(tasksDataSourceRO, "PRAGMA journal_mode")).isEqualToIgnoringCase("wal")

      assertThat(queryInt(dataSourceRO, "SELECT COUNT(*) FROM flyway_schema_history")).isGreaterThan(0)
      assertThat(queryInt(tasksDataSourceRO, "SELECT COUNT(*) FROM flyway_schema_history")).isGreaterThan(0)
    }
  }

  @SpringBootTest
  @ActiveProfiles("test", "memorydb")
  @Nested
  inner class MemoryMode(
    @Autowired private val dataSourceRW: DataSource,
    @Autowired @Qualifier("sqliteDataSourceRO") private val dataSourceRO: DataSource,
    @Autowired @Qualifier("tasksDataSourceRW") private val tasksDataSourceRW: DataSource,
    @Autowired @Qualifier("tasksDataSourceRO") private val tasksDataSourceRO: DataSource,
  ) {
    @Test
    fun `given wal mode when autoriwiring beans then bean instances are the same between RW and RO`() {
      assertThat(dataSourceRW).isSameAs(dataSourceRO)
      assertThat(tasksDataSourceRW).isSameAs(tasksDataSourceRO)
    }
  }
}
