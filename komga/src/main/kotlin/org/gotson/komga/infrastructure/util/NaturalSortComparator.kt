package org.gotson.komga.infrastructure.util

import com.ibm.icu.text.Collator
import com.ibm.icu.text.RuleBasedCollator
import java.util.Comparator

/**
 * Natural sort comparator using ICU4J RuleBasedCollator.
 * Supports internationalization and natural ordering of numbers.
 */
object NaturalSortComparator : Comparator<String> {

  private val collator: RuleBasedCollator by lazy {
    val baseCollator = Collator.getInstance() as RuleBasedCollator
    baseCollator.apply {
      strength = Collator.SECONDARY // Case insensitive (ignores case but considers accents)
      isAlternateHandlingShifted = true // Enable alternate handling for better natural sorting
      setNumericCollation(true) // Ensure numeric collation for proper number sorting
    }
  }

  override fun compare(o1: String?, o2: String?): Int {
    if (o1 == null && o2 == null) return 0
    if (o1 == null) return -1
    if (o2 == null) return 1
    return collator.compare(o1, o2)
  }
}
