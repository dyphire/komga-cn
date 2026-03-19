package org.gotson.komga.interfaces.api.rest

import com.fasterxml.jackson.databind.ObjectMapper
import com.fasterxml.jackson.databind.node.ArrayNode
import com.fasterxml.jackson.databind.node.ObjectNode
import org.springframework.core.io.ClassPathResource

private const val SNAPSHOT_ROOT = "compatibility-snapshots/rest/"

fun assertJsonSnapshot(
  objectMapper: ObjectMapper,
  snapshotName: String,
  actualJson: String,
) {
  val expectedResource = ClassPathResource("$SNAPSHOT_ROOT$snapshotName")
  require(expectedResource.exists()) { "Missing snapshot resource: $SNAPSHOT_ROOT$snapshotName" }

  val expectedJson = expectedResource.inputStream.bufferedReader(Charsets.UTF_8).use { it.readText() }
  val expectedTree = objectMapper.readTree(expectedJson).normalizeForSnapshot()
  val actualTree = objectMapper.readTree(actualJson).normalizeForSnapshot()

  if (actualTree != expectedTree) {
    val prettyPrinter = objectMapper.writerWithDefaultPrettyPrinter()
    error(
      buildString {
        appendLine("Snapshot mismatch: $snapshotName")
        appendLine("Expected:")
        appendLine(prettyPrinter.writeValueAsString(expectedTree))
        appendLine("Actual:")
        appendLine(prettyPrinter.writeValueAsString(actualTree))
      },
    )
  }
}

private fun com.fasterxml.jackson.databind.JsonNode.normalizeForSnapshot(): com.fasterxml.jackson.databind.JsonNode {
  when (this) {
    is ObjectNode -> {
      fieldNames().forEachRemaining { fieldName ->
        replace(fieldName, get(fieldName).normalizeForSnapshot())
      }
    }
    is ArrayNode -> {
      for (index in 0 until size()) {
        set(index, get(index).normalizeForSnapshot())
      }
    }
  }

  if (this is ObjectNode && has("violations") && get("violations") is ArrayNode) {
    val sortedViolations =
      get("violations")
        .map { it.normalizeForSnapshot() }
        .sortedWith(compareBy({ it.path("fieldName").asText() }, { it.path("message").asText() }))

    val normalizedViolations = objectNode().arrayNode()
    sortedViolations.forEach { normalizedViolations.add(it) }
    replace("violations", normalizedViolations)
  }

  return this
}
