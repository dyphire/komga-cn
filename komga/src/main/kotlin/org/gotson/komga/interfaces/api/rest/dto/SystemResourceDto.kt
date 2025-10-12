package org.gotson.komga.interfaces.api.rest.dto

data class SystemResourceDto(
  val memoryUsage: Double,
  val cpuUsage: Double,
  val dbConnectionUsage: Double,
  val circuitBreakerState: String,
  val consecutiveFailures: Int,
  val processingMode: String,
  val isHighLoad: Boolean
)
