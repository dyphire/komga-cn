package org.gotson.komga.infrastructure.monitoring

import io.github.oshai.kotlinlogging.KotlinLogging
import org.gotson.komga.infrastructure.configuration.KomgaProperties
import org.springframework.stereotype.Service
import java.lang.management.ManagementFactory
import java.lang.management.MemoryMXBean
import java.lang.management.OperatingSystemMXBean
import javax.sql.DataSource

private val logger = KotlinLogging.logger {}

@Service
class SystemResourceMonitor(
  private val komgaProperties: KomgaProperties,
  private val dataSource: DataSource,
) {
  private val memoryMXBean: MemoryMXBean = ManagementFactory.getMemoryMXBean()
  private val osMXBean: OperatingSystemMXBean = ManagementFactory.getOperatingSystemMXBean()

  /**
   * Circuit breaker state
   */
  enum class CircuitState {
    CLOSED,    // Normal operation
    OPEN,      // Circuit is open, fallback to single-threaded processing
    HALF_OPEN  // Testing if service recovered
  }

  private var circuitState: CircuitState = CircuitState.CLOSED
  private var lastFailureTime: Long = 0
  private var consecutiveFailures: Int = 0

  /**
   * Check if system resources are under high load
   */
  fun isHighLoad(): Boolean {
    if (!komgaProperties.analysis.circuitBreakerEnabled) {
      return false
    }

    val memoryUsage = getMemoryUsage()
    val cpuUsage = getCpuUsage()
    val dbConnectionUsage = getDbConnectionUsage()

    val highLoad = memoryUsage > komgaProperties.analysis.memoryThreshold ||
                   cpuUsage > komgaProperties.analysis.cpuThreshold ||
                   dbConnectionUsage > komgaProperties.analysis.dbConnectionThreshold

    if (highLoad) {
      consecutiveFailures++
      logger.warn {
        "High system load detected: memory=${"%.2f".format(memoryUsage)}, " +
        "cpu=${"%.2f".format(cpuUsage)}, db=${"%.2f".format(dbConnectionUsage)}"
      }
    } else {
      consecutiveFailures = 0
    }

    return highLoad
  }

  /**
   * Get current circuit breaker state and decide processing mode
   */
  fun getProcessingMode(): ProcessingMode {
    val currentTime = System.currentTimeMillis()

    when (circuitState) {
      CircuitState.CLOSED -> {
        if (isHighLoad()) {
          circuitState = CircuitState.OPEN
          lastFailureTime = currentTime
          logger.warn { "Circuit breaker opened due to high system load" }
          return ProcessingMode.SINGLE_THREADED
        }
        return ProcessingMode.CONCURRENT
      }

      CircuitState.OPEN -> {
        if (currentTime - lastFailureTime > komgaProperties.analysis.circuitBreakerCooldownMs.toMillis()) {
          circuitState = CircuitState.HALF_OPEN
          logger.info { "Circuit breaker moving to half-open state for testing" }
          return ProcessingMode.CONCURRENT_LIMITED
        }
        return ProcessingMode.SINGLE_THREADED
      }

      CircuitState.HALF_OPEN -> {
        if (isHighLoad()) {
          circuitState = CircuitState.OPEN
          lastFailureTime = currentTime
          logger.warn { "Circuit breaker opened again during half-open test" }
          return ProcessingMode.SINGLE_THREADED
        } else {
          circuitState = CircuitState.CLOSED
          consecutiveFailures = 0
          logger.info { "Circuit breaker closed - system recovered" }
          return ProcessingMode.CONCURRENT
        }
      }
    }
  }

  /**
   * Get memory usage ratio (0.0 to 1.0)
   */
  private fun getMemoryUsage(): Double {
    val heapMemory = memoryMXBean.heapMemoryUsage
    return heapMemory.used.toDouble() / heapMemory.max.toDouble()
  }

  /**
   * Get CPU usage ratio (0.0 to 1.0)
   * Note: This is a simple implementation. For production use,
   * consider using a more sophisticated CPU monitoring library.
   */
  private fun getCpuUsage(): Double {
    return try {
      if (osMXBean is com.sun.management.OperatingSystemMXBean) {
        osMXBean.processCpuLoad
      } else {
        // Fallback: use system load average as rough indicator
        val loadAverage = osMXBean.systemLoadAverage
        val availableProcessors = osMXBean.availableProcessors.toDouble()
        if (loadAverage >= 0) {
          (loadAverage / availableProcessors).coerceIn(0.0, 1.0)
        } else {
          0.0
        }
      }
    } catch (e: Exception) {
      logger.debug(e) { "Failed to get CPU usage" }
      0.0
    }
  }

  /**
   * Get database connection pool usage ratio (0.0 to 1.0)
   */
  private fun getDbConnectionUsage(): Double {
    return try {
      when (dataSource) {
        is com.zaxxer.hikari.HikariDataSource -> {
          val active = dataSource.hikariPoolMXBean.activeConnections
          val total = dataSource.maximumPoolSize
          if (total > 0) active.toDouble() / total.toDouble() else 0.0
        }
        else -> {
          // Unknown data source type, assume low usage
          logger.debug { "Unknown DataSource type: ${dataSource.javaClass.name}" }
          0.0
        }
      }
    } catch (e: Exception) {
      logger.debug(e) { "Failed to get DB connection usage" }
      0.0
    }
  }

  /**
   * Get current system metrics for monitoring
   */
  fun getSystemMetrics(): SystemMetrics {
    return SystemMetrics(
      memoryUsage = getMemoryUsage(),
      cpuUsage = getCpuUsage(),
      dbConnectionUsage = getDbConnectionUsage(),
      circuitState = circuitState,
      consecutiveFailures = consecutiveFailures
    )
  }

  /**
   * Processing mode recommendations
   */
  enum class ProcessingMode {
    CONCURRENT,         // Full concurrent processing allowed
    CONCURRENT_LIMITED, // Limited concurrent processing (testing phase)
    SINGLE_THREADED     // Fallback to single-threaded processing
  }

  /**
   * System metrics data class
   */
  data class SystemMetrics(
    val memoryUsage: Double,
    val cpuUsage: Double,
    val dbConnectionUsage: Double,
    val circuitState: CircuitState,
    val consecutiveFailures: Int
  )
}
