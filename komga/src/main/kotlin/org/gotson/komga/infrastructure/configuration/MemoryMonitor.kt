package org.gotson.komga.infrastructure.configuration

import io.github.oshai.kotlinlogging.KotlinLogging
import org.springframework.scheduling.annotation.Scheduled
import org.springframework.stereotype.Component
import java.lang.management.ManagementFactory
import java.lang.management.MemoryNotificationInfo
import java.lang.management.MemoryType
import javax.management.NotificationEmitter
import javax.management.NotificationListener

private val logger = KotlinLogging.logger {}

@Component
class MemoryMonitor(
  private val komgaProperties: KomgaProperties,
) {

  private val memoryMXBean = ManagementFactory.getMemoryMXBean()
  private val memoryPoolMXBeans = ManagementFactory.getMemoryPoolMXBeans()

  init {
    if (komgaProperties.analysis.memory.enableMonitoring) {
      setupMemoryMonitoring()
      logger.info { "Memory monitoring enabled with warning threshold: ${komgaProperties.analysis.memory.heapUsageWarningThreshold * 100}%, critical threshold: ${komgaProperties.analysis.memory.heapUsageCriticalThreshold * 100}%" }
    }
  }

  private fun setupMemoryMonitoring() {
    // Set heap memory usage threshold monitoring
    memoryPoolMXBeans.forEach { pool ->
      if (pool.type == MemoryType.HEAP) {
        try {
          val warningThreshold = (pool.usage.max * komgaProperties.analysis.memory.heapUsageWarningThreshold).toLong()
          val criticalThreshold = (pool.usage.max * komgaProperties.analysis.memory.heapUsageCriticalThreshold).toLong()

          pool.collectionUsageThreshold = warningThreshold
          pool.usageThreshold = warningThreshold

          if (pool is NotificationEmitter) {
            pool.addNotificationListener(
              object : NotificationListener {
                override fun handleNotification(notification: javax.management.Notification, handback: Any?) {
                  when (notification.type) {
                    MemoryNotificationInfo.MEMORY_THRESHOLD_EXCEEDED -> {
                      val usage = pool.usage
                      val usagePercent = usage.used.toDouble() / usage.max.toDouble()

                      if (usagePercent >= komgaProperties.analysis.memory.heapUsageCriticalThreshold) {
                        logger.warn { "CRITICAL: Heap memory usage exceeded ${usagePercent * 100}%. Used: ${usage.used / 1024 / 1024}MB, Max: ${usage.max / 1024 / 1024}MB" }
                        if (komgaProperties.analysis.memory.enableAggressiveGC) {
                          triggerGarbageCollection()
                        }
                      } else {
                        logger.info { "WARNING: Heap memory usage exceeded ${usagePercent * 100}%. Used: ${usage.used / 1024 / 1024}MB, Max: ${usage.max / 1024 / 1024}MB" }
                      }
                    }
                    MemoryNotificationInfo.MEMORY_COLLECTION_THRESHOLD_EXCEEDED -> {
                      logger.debug { "Memory collection threshold exceeded for pool: ${pool.name}" }
                    }
                  }
                }
              },
              null,  // filter - null means no filtering
              null   // handback - user data passed to listener
            )
          }
        } catch (e: Exception) {
          logger.error(e) { "Failed to setup memory monitoring for pool: ${pool.name}" }
        }
      }
    }
  }

  @Scheduled(fixedDelayString = "#{@komgaProperties.analysis.memory.gcIntervalMs.toMillis()}")
  fun periodicMemoryCheck() {
    if (!komgaProperties.analysis.memory.enableMonitoring) return

    val heapMemory = memoryMXBean.heapMemoryUsage
    val nonHeapMemory = memoryMXBean.nonHeapMemoryUsage
    val heapUsagePercent = heapMemory.used.toDouble() / heapMemory.max.toDouble()

    if (heapUsagePercent >= komgaProperties.analysis.memory.heapUsageCriticalThreshold) {
      logger.warn {
        "PERIODIC CHECK - CRITICAL heap usage: ${String.format("%.1f", heapUsagePercent * 100)}% " +
        "(Used: ${heapMemory.used / 1024 / 1024}MB, Max: ${heapMemory.max / 1024 / 1024}MB, " +
        "Non-heap: ${nonHeapMemory.used / 1024 / 1024}MB)"
      }
    } else if (heapUsagePercent >= komgaProperties.analysis.memory.heapUsageWarningThreshold) {
      logger.info {
        "PERIODIC CHECK - High heap usage: ${String.format("%.1f", heapUsagePercent * 100)}% " +
        "(Used: ${heapMemory.used / 1024 / 1024}MB, Max: ${heapMemory.max / 1024 / 1024}MB)"
      }
    }
  }

  private fun triggerGarbageCollection() {
    logger.info { "Triggering garbage collection due to high memory usage" }
    System.gc()
  }

  fun getMemoryStats(): MemoryStats {
    val heap = memoryMXBean.heapMemoryUsage
    val nonHeap = memoryMXBean.nonHeapMemoryUsage

    return MemoryStats(
      heapUsed = heap.used,
      heapMax = heap.max,
      heapUsagePercent = heap.used.toDouble() / heap.max.toDouble(),
      nonHeapUsed = nonHeap.used,
      nonHeapMax = nonHeap.max,
      nonHeapUsagePercent = if (nonHeap.max > 0) nonHeap.used.toDouble() / nonHeap.max.toDouble() else 0.0
    )
  }

  data class MemoryStats(
    val heapUsed: Long,
    val heapMax: Long,
    val heapUsagePercent: Double,
    val nonHeapUsed: Long,
    val nonHeapMax: Long,
    val nonHeapUsagePercent: Double,
  )
}
