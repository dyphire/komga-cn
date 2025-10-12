package org.gotson.komga.interfaces.api.rest

import io.swagger.v3.oas.annotations.Operation
import io.swagger.v3.oas.annotations.tags.Tag
import org.gotson.komga.infrastructure.monitoring.SystemResourceMonitor
import org.gotson.komga.interfaces.api.rest.dto.SystemResourceDto
import org.springframework.http.MediaType
import org.springframework.web.bind.annotation.GetMapping
import org.springframework.web.bind.annotation.RequestMapping
import org.springframework.web.bind.annotation.RestController

@RestController
@RequestMapping("api/v1/system", produces = [MediaType.APPLICATION_JSON_VALUE])
@Tag(name = "System")
class SystemResourceController(
  private val systemResourceMonitor: SystemResourceMonitor,
) {

  @GetMapping("resources")
  @Operation(summary = "Get system resource usage and circuit breaker status")
  fun getSystemResources(): SystemResourceDto {
    val metrics = systemResourceMonitor.getSystemMetrics()
    val processingMode = systemResourceMonitor.getProcessingMode()

    return SystemResourceDto(
      memoryUsage = metrics.memoryUsage,
      cpuUsage = metrics.cpuUsage,
      dbConnectionUsage = metrics.dbConnectionUsage,
      circuitBreakerState = metrics.circuitState.name,
      consecutiveFailures = metrics.consecutiveFailures,
      processingMode = processingMode.name,
      isHighLoad = systemResourceMonitor.isHighLoad()
    )
  }
}
