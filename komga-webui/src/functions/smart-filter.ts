import {
  SearchConditionBook,
  SearchConditionAllOfBook,
  SearchConditionAnyOfBook,
  SearchConditionReadStatus,
  SearchConditionTag,
  SearchConditionOneShot,
  SearchConditionDeleted,
  SearchConditionMediaProfile,
  SearchConditionAuthor,
  SearchConditionReleaseDate,
  SearchConditionSeries,
  SearchConditionAllOfSeries,
  SearchConditionAnyOfSeries,
  SearchConditionGenre,
  SearchConditionPublisher,
  SearchConditionLanguage,
  SearchConditionAgeRating,
  SearchConditionSeriesStatus,
  SearchConditionComplete,
  SearchOperatorIs,
  SearchOperatorIsNot,
  SearchOperatorIsTrue,
  SearchOperatorIsFalse,
} from '@/types/komga-search'
import { ReadStatus, MediaProfile } from '@/types/enum-books'
import { SeriesStatus } from '@/types/enum-series'

export interface SmartFilterToken {
  type: 'field' | 'operator' | 'value' | 'logic' | 'lparen' | 'rparen'
  value: string
}

export class SmartFilterParser {
  private tokens: SmartFilterToken[] = []
  private position = 0

  parse(query: string): SearchConditionBook | null {
    if (!query.trim()) return null

    this.tokens = this.tokenize(query)
    this.position = 0

    try {
      return this.parseExpression()
    } catch (error) {
      // Parse error occurred, return null
      return null
    }
  }

  private tokenize(query: string): SmartFilterToken[] {
    const tokens: SmartFilterToken[] = []
    const words = query.split(/\s+/)

    for (const word of words) {
      if (word === '(') {
        tokens.push({ type: 'lparen', value: word })
      } else if (word === ')') {
        tokens.push({ type: 'rparen', value: word })
      } else if (word === 'AND' || word === 'OR' || word === 'NOT') {
        tokens.push({ type: 'logic', value: word })
      } else if (word.includes(':')) {
        // Split by first colon to get field and value
        const firstColonIndex = word.indexOf(':')
        const field = word.substring(0, firstColonIndex)
        const value = word.substring(firstColonIndex + 1)

        tokens.push({ type: 'field', value: field })
        tokens.push({ type: 'operator', value: ':' })
        tokens.push({ type: 'value', value: value })
      } else {
        tokens.push({ type: 'value', value: word })
      }
    }

    return tokens
  }

  private parseExpression(): SearchConditionBook | null {
    const conditions: SearchConditionBook[] = []
    let logicOp = 'AND'
    let negateNext = false

    while (this.position < this.tokens.length) {
      const token = this.tokens[this.position]

      if (token.type === 'lparen') {
        this.position++
        const subExpr = this.parseExpression()
        if (subExpr) {
          conditions.push(negateNext ? this.negateCondition(subExpr) : subExpr)
          negateNext = false
        }
        if (this.position < this.tokens.length && this.tokens[this.position].type === 'rparen') {
          this.position++
        }
      } else if (token.type === 'logic') {
        if (token.value === 'NOT') {
          negateNext = true
        } else {
          logicOp = token.value
        }
        this.position++
      } else if (token.type === 'field') {
        const condition = this.parseCondition(negateNext)
        if (condition) {
          conditions.push(condition)
          negateNext = false
        }
      } else {
        this.position++
      }
    }

    if (conditions.length === 0) return null
    if (conditions.length === 1) return conditions[0]

    return logicOp === 'AND'
      ? new SearchConditionAllOfBook(conditions)
      : new SearchConditionAnyOfBook(conditions)
  }

  private negateCondition(condition: SearchConditionBook): SearchConditionBook {
    // This is a simplified negation - in practice, we'd need to implement
    // proper negation for each condition type
    // For now, return the original condition
    return condition
  }

  private parseCondition(negate: boolean = false): SearchConditionBook | null {
    if (this.position >= this.tokens.length) return null

    const fieldToken = this.tokens[this.position++]
    if (fieldToken.type !== 'field') return null

    if (this.position >= this.tokens.length) return null
    const opToken = this.tokens[this.position++]
    if (opToken.type !== 'operator') return null

    if (this.position >= this.tokens.length) return null
    const valueToken = this.tokens[this.position++]
    if (valueToken.type !== 'value') return null

    return this.createCondition(fieldToken.value, opToken.value, valueToken.value, negate)
  }

  private createCondition(field: string, operator: string, value: string, negate: boolean = false): SearchConditionBook | null {
    const normalizedValue = value.toLowerCase()

    switch (field.toLowerCase()) {
      case 'read':
      case 'readstatus':
        return this.createReadStatusCondition(operator, normalizedValue, negate)

      case 'tag':
      case 'tags':
        return this.createTagCondition(operator, value, negate)

      case 'oneshot':
        return this.createOneShotCondition(operator, normalizedValue, negate)

      case 'deleted':
        return this.createDeletedCondition(operator, normalizedValue, negate)

      case 'mediaprofile':
        return this.createMediaProfileCondition(operator, normalizedValue, negate)

      case 'author':
      case 'authors':
      case 'writer':
      case 'penciller':
      case 'letterer':
      case 'inker':
      case 'editor':
      case 'cover':
      case 'colorist':
        return this.createAuthorCondition(operator, value, field, negate)

      case 'releasedate':
        return this.createReleaseDateCondition(operator, value, negate)

      default:
        return null
    }
  }

  private createReadStatusCondition(operator: string, value: string, negate: boolean = false): SearchConditionBook | null {
    let readStatus: ReadStatus | null = null

    if (value === 'true' || value === 'read') readStatus = ReadStatus.READ
    else if (value === 'false' || value === 'unread') readStatus = ReadStatus.UNREAD
    else if (value === 'inprogress' || value === 'in_progress') readStatus = ReadStatus.IN_PROGRESS

    if (!readStatus) return null

    switch (operator) {
      case ':':
        return new SearchConditionReadStatus(negate ? new SearchOperatorIsNot(readStatus) : new SearchOperatorIs(readStatus))
      default:
        return null
    }
  }

  private createTagCondition(operator: string, value: string, negate: boolean = false): SearchConditionBook | null {
    switch (operator) {
      case ':':
        return new SearchConditionTag(negate ? new SearchOperatorIsNot(value) : new SearchOperatorIs(value))
      default:
        return null
    }
  }

  private createOneShotCondition(operator: string, value: string, negate: boolean = false): SearchConditionBook | null {
    const isTrue = value === 'true'

    switch (operator) {
      case ':':
        if (negate) {
          return new SearchConditionOneShot(isTrue ? new SearchOperatorIsFalse() : new SearchOperatorIsTrue())
        } else {
          return new SearchConditionOneShot(isTrue ? new SearchOperatorIsTrue() : new SearchOperatorIsFalse())
        }
      default:
        return null
    }
  }

  private createDeletedCondition(operator: string, value: string, negate: boolean = false): SearchConditionBook | null {
    const isTrue = value === 'true'

    switch (operator) {
      case ':':
        if (negate) {
          return new SearchConditionDeleted(isTrue ? new SearchOperatorIsFalse() : new SearchOperatorIsTrue())
        } else {
          return new SearchConditionDeleted(isTrue ? new SearchOperatorIsTrue() : new SearchOperatorIsFalse())
        }
      default:
        return null
    }
  }

  private createMediaProfileCondition(operator: string, value: string, negate: boolean = false): SearchConditionBook | null {
    const profile = Object.values(MediaProfile).find(p => p.toLowerCase() === value)
    if (!profile) return null

    switch (operator) {
      case ':':
        return new SearchConditionMediaProfile(negate ? new SearchOperatorIsNot(profile) : new SearchOperatorIs(profile))
      default:
        return null
    }
  }

  private createAuthorCondition(operator: string, value: string, role?: string, negate: boolean = false): SearchConditionBook | null {
    const authorMatch: any = { name: value }
    if (role && role !== 'author' && role !== 'authors') {
      authorMatch.role = role
    }

    switch (operator) {
      case ':':
        return new SearchConditionAuthor(negate ? new SearchOperatorIsNot(authorMatch) : new SearchOperatorIs(authorMatch))
      default:
        return null
    }
  }

  private createReleaseDateCondition(operator: string, value: string, negate: boolean = false): SearchConditionBook | null {
    // 简化实现，实际应该解析日期
    switch (operator) {
      case ':':
        return new SearchConditionReleaseDate(negate ? new SearchOperatorIsNot(value) : new SearchOperatorIs(value))
      default:
        return null
    }
  }
}

export function parseSmartFilter(query: string): SearchConditionBook | null {
  const parser = new SmartFilterParser()
  return parser.parse(query)
}

export class SmartFilterSeriesParser {
  private tokens: SmartFilterToken[] = []
  private position = 0

  parse(query: string): SearchConditionSeries | null {
    if (!query.trim()) return null

    this.tokens = this.tokenize(query)
    this.position = 0

    try {
      return this.parseExpression()
    } catch (error) {
      // Parse error occurred, return null
      return null
    }
  }

  private tokenize(query: string): SmartFilterToken[] {
    const tokens: SmartFilterToken[] = []
    const words = query.split(/\s+/)

    for (const word of words) {
      if (word === '(') {
        tokens.push({ type: 'lparen', value: word })
      } else if (word === ')') {
        tokens.push({ type: 'rparen', value: word })
      } else if (word === 'AND' || word === 'OR' || word === 'NOT') {
        tokens.push({ type: 'logic', value: word })
      } else if (word.includes(':')) {
        // Split by first colon to get field and value
        const firstColonIndex = word.indexOf(':')
        const field = word.substring(0, firstColonIndex)
        const value = word.substring(firstColonIndex + 1)

        tokens.push({ type: 'field', value: field })
        tokens.push({ type: 'operator', value: ':' })
        tokens.push({ type: 'value', value: value })
      } else {
        tokens.push({ type: 'value', value: word })
      }
    }

    return tokens
  }

  private parseExpression(): SearchConditionSeries | null {
    const conditions: SearchConditionSeries[] = []
    let logicOp = 'AND'
    let negateNext = false

    while (this.position < this.tokens.length) {
      const token = this.tokens[this.position]

      if (token.type === 'lparen') {
        this.position++
        const subExpr = this.parseExpression()
        if (subExpr) {
          conditions.push(negateNext ? this.negateCondition(subExpr) : subExpr)
          negateNext = false
        }
        if (this.position < this.tokens.length && this.tokens[this.position].type === 'rparen') {
          this.position++
        }
      } else if (token.type === 'logic') {
        if (token.value === 'NOT') {
          negateNext = true
        } else {
          logicOp = token.value
        }
        this.position++
      } else if (token.type === 'field') {
        const condition = this.parseCondition(negateNext)
        if (condition) {
          conditions.push(condition)
          negateNext = false
        }
      } else {
        this.position++
      }
    }

    if (conditions.length === 0) return null
    if (conditions.length === 1) return conditions[0]

    return logicOp === 'AND'
      ? new SearchConditionAllOfSeries(conditions)
      : new SearchConditionAnyOfSeries(conditions)
  }

  private negateCondition(condition: SearchConditionSeries): SearchConditionSeries {
    // This is a simplified negation - in practice, we'd need to implement
    // proper negation for each condition type
    // For now, return the original condition
    return condition
  }

  private parseCondition(negate: boolean = false): SearchConditionSeries | null {
    if (this.position >= this.tokens.length) return null

    const fieldToken = this.tokens[this.position++]
    if (fieldToken.type !== 'field') return null

    if (this.position >= this.tokens.length) return null
    const opToken = this.tokens[this.position++]
    if (opToken.type !== 'operator') return null

    if (this.position >= this.tokens.length) return null
    const valueToken = this.tokens[this.position++]
    if (valueToken.type !== 'value') return null

    return this.createCondition(fieldToken.value, opToken.value, valueToken.value, negate)
  }

  private createCondition(field: string, operator: string, value: string, negate: boolean = false): SearchConditionSeries | null {
    const normalizedValue = value.toLowerCase()

    switch (field.toLowerCase()) {
      case 'genre':
      case 'genres':
        return this.createGenreCondition(operator, value, negate)

      case 'tag':
      case 'tags':
        return this.createTagCondition(operator, value, negate)

      case 'publisher':
        return this.createPublisherCondition(operator, value, negate)

      case 'language':
        return this.createLanguageCondition(operator, value, negate)

      case 'agerating':
      case 'age_rating':
        return this.createAgeRatingCondition(operator, value, negate)

      case 'status':
        return this.createStatusCondition(operator, normalizedValue, negate)

      case 'complete':
        return this.createCompleteCondition(operator, normalizedValue, negate)

      case 'oneshot':
        return this.createOneShotCondition(operator, normalizedValue, negate)

      case 'deleted':
        return this.createDeletedCondition(operator, normalizedValue, negate)

      case 'author':
      case 'authors':
      case 'writer':
      case 'penciller':
      case 'letterer':
      case 'inker':
      case 'editor':
      case 'cover':
      case 'colorist':
        return this.createAuthorCondition(operator, value, field, negate)

      case 'releasedate':
      case 'release_date':
        return this.createReleaseDateCondition(operator, value, negate)

      default:
        return null
    }
  }

  private createGenreCondition(operator: string, value: string, negate: boolean = false): SearchConditionSeries | null {
    switch (operator) {
      case ':':
        return new SearchConditionGenre(negate ? new SearchOperatorIsNot(value) : new SearchOperatorIs(value))
      default:
        return null
    }
  }

  private createTagCondition(operator: string, value: string, negate: boolean = false): SearchConditionSeries | null {
    switch (operator) {
      case ':':
        return new SearchConditionTag(negate ? new SearchOperatorIsNot(value) : new SearchOperatorIs(value))
      default:
        return null
    }
  }

  private createPublisherCondition(operator: string, value: string, negate: boolean = false): SearchConditionSeries | null {
    switch (operator) {
      case ':':
        return new SearchConditionPublisher(negate ? new SearchOperatorIsNot(value) : new SearchOperatorIs(value))
      default:
        return null
    }
  }

  private createLanguageCondition(operator: string, value: string, negate: boolean = false): SearchConditionSeries | null {
    switch (operator) {
      case ':':
        return new SearchConditionLanguage(negate ? new SearchOperatorIsNot(value) : new SearchOperatorIs(value))
      default:
        return null
    }
  }

  private createAgeRatingCondition(operator: string, value: string, negate: boolean = false): SearchConditionSeries | null {
    const rating = parseInt(value)
    if (isNaN(rating)) return null

    switch (operator) {
      case ':':
        return new SearchConditionAgeRating(negate ? new SearchOperatorIsNot(rating) : new SearchOperatorIs(rating))
      default:
        return null
    }
  }

  private createStatusCondition(operator: string, value: string, negate: boolean = false): SearchConditionSeries | null {
    let status: SeriesStatus | null = null

    if (value === 'ongoing') status = SeriesStatus.ONGOING
    else if (value === 'ended') status = SeriesStatus.ENDED
    else if (value === 'abandoned') status = SeriesStatus.ABANDONED
    else if (value === 'hiatus') status = SeriesStatus.HIATUS

    if (!status) return null

    switch (operator) {
      case ':':
        return new SearchConditionSeriesStatus(negate ? new SearchOperatorIsNot(status) : new SearchOperatorIs(status))
      default:
        return null
    }
  }

  private createCompleteCondition(operator: string, value: string, negate: boolean = false): SearchConditionSeries | null {
    const isTrue = value === 'true'

    switch (operator) {
      case ':':
        if (negate) {
          return new SearchConditionComplete(isTrue ? new SearchOperatorIsFalse() : new SearchOperatorIsTrue())
        } else {
          return new SearchConditionComplete(isTrue ? new SearchOperatorIsTrue() : new SearchOperatorIsFalse())
        }
      default:
        return null
    }
  }

  private createOneShotCondition(operator: string, value: string, negate: boolean = false): SearchConditionSeries | null {
    const isTrue = value === 'true'

    switch (operator) {
      case ':':
        if (negate) {
          return new SearchConditionOneShot(isTrue ? new SearchOperatorIsFalse() : new SearchOperatorIsTrue())
        } else {
          return new SearchConditionOneShot(isTrue ? new SearchOperatorIsTrue() : new SearchOperatorIsFalse())
        }
      default:
        return null
    }
  }

  private createDeletedCondition(operator: string, value: string, negate: boolean = false): SearchConditionSeries | null {
    const isTrue = value === 'true'

    switch (operator) {
      case ':':
        if (negate) {
          return new SearchConditionDeleted(isTrue ? new SearchOperatorIsFalse() : new SearchOperatorIsTrue())
        } else {
          return new SearchConditionDeleted(isTrue ? new SearchOperatorIsTrue() : new SearchOperatorIsFalse())
        }
      default:
        return null
    }
  }

  private createAuthorCondition(operator: string, value: string, role?: string, negate: boolean = false): SearchConditionSeries | null {
    const authorMatch: any = { name: value }
    if (role && role !== 'author' && role !== 'authors') {
      authorMatch.role = role
    }

    switch (operator) {
      case ':':
        return new SearchConditionAuthor(negate ? new SearchOperatorIsNot(authorMatch) : new SearchOperatorIs(authorMatch))
      default:
        return null
    }
  }

  private createReleaseDateCondition(operator: string, value: string, negate: boolean = false): SearchConditionSeries | null {
    // 简化实现，实际应该解析日期
    switch (operator) {
      case ':':
        return new SearchConditionReleaseDate(negate ? new SearchOperatorIsNot(value) : new SearchOperatorIs(value))
      default:
        return null
    }
  }
}

export function parseSmartFilterForSeries(query: string): SearchConditionSeries | null {
  const parser = new SmartFilterSeriesParser()
  return parser.parse(query)
}
