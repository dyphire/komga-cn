package org.gotson.komga.interfaces.sse

import com.fasterxml.jackson.databind.ObjectMapper
import com.fasterxml.jackson.databind.node.ObjectNode
import org.springframework.core.io.ClassPathResource

private const val SNAPSHOT_ROOT = "compatibility-snapshots/sse/"

fun assertSseSnapshot(
  objectMapper: ObjectMapper,
  snapshotName: String,
  actual: SseEventDescriptor,
) {
  val expectedResource = ClassPathResource("$SNAPSHOT_ROOT$snapshotName")
  require(expectedResource.exists()) { "Missing snapshot resource: $SNAPSHOT_ROOT$snapshotName" }

  val expectedJson = expectedResource.inputStream.bufferedReader(Charsets.UTF_8).use { it.readText() }
  val expectedTree = objectMapper.readTree(expectedJson)
  val actualTree = objectMapper.valueToTree<ObjectNode>(actual).normalizeForSnapshot(objectMapper)

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

private fun ObjectNode.normalizeForSnapshot(objectMapper: ObjectMapper): ObjectNode {
  replace("payload", objectMapper.valueToTree(get("payload")))
  return this
}
