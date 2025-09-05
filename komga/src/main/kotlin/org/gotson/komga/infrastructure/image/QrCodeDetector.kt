package org.gotson.komga.infrastructure.image

import com.google.zxing.BinaryBitmap
import com.google.zxing.BarcodeFormat
import com.google.zxing.DecodeHintType
import com.google.zxing.MultiFormatReader
import com.google.zxing.NotFoundException
import com.google.zxing.RGBLuminanceSource
import com.google.zxing.common.HybridBinarizer
import io.github.oshai.kotlinlogging.KotlinLogging
import org.springframework.stereotype.Service
import java.util.EnumSet
import java.awt.RenderingHints
import java.awt.image.BufferedImage
import javax.imageio.ImageIO

private val logger = KotlinLogging.logger {}

@Service
class QrCodeDetector {

  private val qrCodeWhiteList = listOf(
    Regex("^https://[^.]+\\.fanbox\\.cc"),
    Regex("^https://twitter\\.com"),
    Regex("^https://x\\.com"),
    Regex("^https://www\\.pixiv\\.net"),
    Regex("^https://www\\.dmm\\.co\\.jp"),
    Regex("^https://fantia\\.jp"),
    Regex("^https://marshmallow-qa\\.com"),
    Regex("^https://www\\.dlsite\\.com"),
    Regex("^https://hitomi\\.la")
  )

  private val hints =
    mapOf(
      DecodeHintType.POSSIBLE_FORMATS to EnumSet.of(BarcodeFormat.QR_CODE),
      DecodeHintType.TRY_HARDER to true
    )

  fun containsQrCode(imageBytes: ByteArray): Boolean {
    return try {
      val image = ImageIO.read(imageBytes.inputStream()) ?: return false
      val scaled = resizeImage(image)

      if (!isColorImage(scaled)) {
        logger.debug { "Image is grayscale, skipping QR detection" }
        return false
      }

      val luminanceSource = RGBLuminanceSource(scaled.width, scaled.height, getPixels(scaled))
      val binaryBitmap = BinaryBitmap(HybridBinarizer(luminanceSource))

      val result = MultiFormatReader().decode(binaryBitmap, hints)

      val isWhitelisted = qrCodeWhiteList.any { it.containsMatchIn(result.text) }
      if (isWhitelisted) {
        logger.debug { "QR code is in whitelist, ignoring: ${result.text}" }
        return false
      }

      logger.debug { "Found QR code: ${result.text}" }
      true
    } catch (e: NotFoundException) {
      false
    } catch (e: Exception) {
      logger.error(e) { "Error while detecting QR code" }
      false
    }
  }

  private fun getPixels(image: BufferedImage): IntArray {
    val pixels = IntArray(image.width * image.height)
    image.getRGB(0, 0, image.width, image.height, pixels, 0, image.width)
    return pixels
  }

  private fun resizeImage(image: BufferedImage, maxWidth: Int = 1000): BufferedImage {
    if (image.width <= maxWidth) return image

    val ratio = maxWidth.toDouble() / image.width
    val newHeight = (image.height * ratio).toInt()
    val scaled = BufferedImage(maxWidth, newHeight, BufferedImage.TYPE_INT_RGB)

    val g = scaled.createGraphics()
    g.setRenderingHint(RenderingHints.KEY_INTERPOLATION, RenderingHints.VALUE_INTERPOLATION_BILINEAR)
    g.setRenderingHint(RenderingHints.KEY_RENDERING, RenderingHints.VALUE_RENDER_QUALITY)
    g.drawImage(image, 0, 0, maxWidth, newHeight, null)
    g.dispose()

    return scaled
  }

  private fun isColorImage(image: BufferedImage, step: Int = 4): Boolean {
    val pixels = getPixels(image)
    for (i in pixels.indices step step) {
      val p = pixels[i]
      val r = (p shr 16) and 0xFF
      val g = (p shr 8) and 0xFF
      val b = p and 0xFF
      if (!(r == g && g == b)) {
        return true
      }
    }
    return false
  }
}
