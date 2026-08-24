use std::borrow::Cow;
use std::collections::VecDeque;
use std::mem;

use tantivy::Index;
use tantivy::schema::{IndexRecordOption, TextFieldIndexing, TextOptions};
use tantivy::tokenizer::{
    AsciiFoldingFilter, LowerCaser, RawTokenizer, SimpleTokenizer, TextAnalyzer, Token,
    TokenFilter, TokenStream, Tokenizer,
};

pub const SEARCH_ANALYZER_VERSION: u32 = 6;

pub fn search_analyzer_version() -> u32 {
    SEARCH_ANALYZER_VERSION
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SearchFieldClass {
    MultilingualFullText,
    ExactTerm,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SearchAnalyzerPhase {
    Index,
    Query,
}

pub fn search_text_field_options(class: SearchFieldClass) -> TextOptions {
    TextOptions::default().set_stored().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer(&index_tokenizer_profile_name(class))
            .set_index_option(index_record_option(class)),
    )
}

pub fn register_search_analyzers(index: &Index) {
    for class in [
        SearchFieldClass::MultilingualFullText,
        SearchFieldClass::ExactTerm,
    ] {
        index.tokenizers().register(
            &index_tokenizer_profile_name(class),
            build_index_time_analyzer(class),
        );
        index.tokenizers().register(
            &query_tokenizer_profile_name(class),
            build_query_time_analyzer(class),
        );
    }
}

pub fn index_tokenizer_profile_name(class: SearchFieldClass) -> String {
    tokenizer_profile_name(class, SearchAnalyzerPhase::Index)
}

pub fn query_tokenizer_profile_name(class: SearchFieldClass) -> String {
    tokenizer_profile_name(class, SearchAnalyzerPhase::Query)
}

pub fn build_index_time_analyzer(class: SearchFieldClass) -> TextAnalyzer {
    match class {
        SearchFieldClass::MultilingualFullText => build_multilingual_index_text_analyzer(),
        SearchFieldClass::ExactTerm => build_exact_term_analyzer(),
    }
}

pub fn build_query_time_analyzer(class: SearchFieldClass) -> TextAnalyzer {
    match class {
        SearchFieldClass::MultilingualFullText => build_default_text_analyzer(),
        SearchFieldClass::ExactTerm => build_exact_term_analyzer(),
    }
}

fn index_record_option(class: SearchFieldClass) -> IndexRecordOption {
    match class {
        SearchFieldClass::MultilingualFullText => IndexRecordOption::WithFreqsAndPositions,
        SearchFieldClass::ExactTerm => IndexRecordOption::Basic,
    }
}

fn tokenizer_profile_name(class: SearchFieldClass, phase: SearchAnalyzerPhase) -> String {
    let field_class = match class {
        SearchFieldClass::MultilingualFullText => "multilingual",
        SearchFieldClass::ExactTerm => "exact_term",
    };
    let phase = match phase {
        SearchAnalyzerPhase::Index => "index",
        SearchAnalyzerPhase::Query => "query",
    };

    format!("komga_{field_class}_{phase}_v{SEARCH_ANALYZER_VERSION}")
}

fn build_default_text_analyzer() -> TextAnalyzer {
    TextAnalyzer::builder(SimpleTokenizer::default())
        .filter(MultilingualWidthNormalizer)
        .filter(LowerCaser)
        .filter(CjkBigramApproximationFilter)
        .filter(AsciiFoldingFilter)
        .build()
}

fn build_multilingual_index_text_analyzer() -> TextAnalyzer {
    TextAnalyzer::builder(SimpleTokenizer::default())
        .filter(MultilingualWidthNormalizer)
        .filter(LowerCaser)
        .filter(CjkBigramApproximationFilter)
        .filter(StagedNgramApproximationFilter::new(3, 10, true))
        .filter(AsciiFoldingFilter)
        .build()
}

fn build_exact_term_analyzer() -> TextAnalyzer {
    TextAnalyzer::builder(RawTokenizer::default())
        .filter(LowerCaser)
        .build()
}

#[derive(Clone)]
struct CjkBigramApproximationFilter;

impl TokenFilter for CjkBigramApproximationFilter {
    type Tokenizer<T: Tokenizer> = CjkBigramApproximationTokenizer<T>;

    fn transform<T: Tokenizer>(self, tokenizer: T) -> Self::Tokenizer<T> {
        CjkBigramApproximationTokenizer {
            tokenizer,
            pending: VecDeque::new(),
            current: Token::default(),
        }
    }
}

#[derive(Clone)]
struct CjkBigramApproximationTokenizer<T> {
    tokenizer: T,
    pending: VecDeque<Token>,
    current: Token,
}

impl<T: Tokenizer> Tokenizer for CjkBigramApproximationTokenizer<T> {
    type TokenStream<'a> = CjkBigramApproximationTokenStream<'a, T::TokenStream<'a>>;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        self.pending.clear();
        CjkBigramApproximationTokenStream {
            tail: self.tokenizer.token_stream(text),
            pending: &mut self.pending,
            current: &mut self.current,
        }
    }
}

struct CjkBigramApproximationTokenStream<'a, T> {
    tail: T,
    pending: &'a mut VecDeque<Token>,
    current: &'a mut Token,
}

impl<T: TokenStream> TokenStream for CjkBigramApproximationTokenStream<'_, T> {
    fn advance(&mut self) -> bool {
        loop {
            if let Some(token) = self.pending.pop_front() {
                *self.current = token;
                return true;
            }

            if !self.tail.advance() {
                return false;
            }

            self.pending
                .extend(cjk_bigram_approximation_tokens(self.tail.token()));
        }
    }

    fn token(&self) -> &Token {
        self.current
    }

    fn token_mut(&mut self) -> &mut Token {
        self.current
    }
}

#[derive(Clone)]
struct StagedNgramApproximationFilter {
    min_gram: usize,
    max_gram: usize,
    preserve_original: bool,
}

impl StagedNgramApproximationFilter {
    fn new(min_gram: usize, max_gram: usize, preserve_original: bool) -> Self {
        Self {
            min_gram,
            max_gram,
            preserve_original,
        }
    }
}

impl TokenFilter for StagedNgramApproximationFilter {
    type Tokenizer<T: Tokenizer> = StagedNgramApproximationTokenizer<T>;

    fn transform<T: Tokenizer>(self, tokenizer: T) -> Self::Tokenizer<T> {
        StagedNgramApproximationTokenizer {
            tokenizer,
            pending: VecDeque::new(),
            current: Token::default(),
            min_gram: self.min_gram,
            max_gram: self.max_gram,
            preserve_original: self.preserve_original,
        }
    }
}

#[derive(Clone)]
struct StagedNgramApproximationTokenizer<T> {
    tokenizer: T,
    pending: VecDeque<Token>,
    current: Token,
    min_gram: usize,
    max_gram: usize,
    preserve_original: bool,
}

impl<T: Tokenizer> Tokenizer for StagedNgramApproximationTokenizer<T> {
    type TokenStream<'a> = StagedNgramApproximationTokenStream<'a, T::TokenStream<'a>>;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        self.pending.clear();
        StagedNgramApproximationTokenStream {
            tail: self.tokenizer.token_stream(text),
            pending: &mut self.pending,
            current: &mut self.current,
            min_gram: self.min_gram,
            max_gram: self.max_gram,
            preserve_original: self.preserve_original,
        }
    }
}

struct StagedNgramApproximationTokenStream<'a, T> {
    tail: T,
    pending: &'a mut VecDeque<Token>,
    current: &'a mut Token,
    min_gram: usize,
    max_gram: usize,
    preserve_original: bool,
}

impl<T: TokenStream> TokenStream for StagedNgramApproximationTokenStream<'_, T> {
    fn advance(&mut self) -> bool {
        loop {
            if let Some(token) = self.pending.pop_front() {
                *self.current = token;
                return true;
            }

            if !self.tail.advance() {
                return false;
            }

            self.pending.extend(staged_ngram_approximation_tokens(
                self.tail.token(),
                self.min_gram,
                self.max_gram,
                self.preserve_original,
            ));
        }
    }

    fn token(&self) -> &Token {
        self.current
    }

    fn token_mut(&mut self) -> &mut Token {
        self.current
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SegmentKind {
    Cjk,
    Other,
}

struct TokenSegment {
    kind: SegmentKind,
    start: usize,
    end: usize,
}

fn cjk_bigram_approximation_tokens(token: &Token) -> Vec<Token> {
    let segments = split_token_segments(&token.text);
    let mut tokens = Vec::new();

    for segment in segments {
        match segment.kind {
            SegmentKind::Cjk => tokens.extend(cjk_bigram_segment_tokens(token, &segment)),
            SegmentKind::Other => tokens.push(segment_token(token, segment.start, segment.end)),
        }
    }

    if tokens.is_empty() {
        vec![token.clone()]
    } else {
        tokens
    }
}

fn staged_ngram_approximation_tokens(
    token: &Token,
    min_gram: usize,
    max_gram: usize,
    preserve_original: bool,
) -> Vec<Token> {
    let chars = token.text.char_indices().collect::<Vec<_>>();
    if chars.is_empty() {
        return Vec::new();
    }

    let mut tokens = Vec::new();
    if preserve_original {
        tokens.push(token.clone());
    }

    for start in 0..chars.len() {
        for gram in min_gram..=max_gram {
            let end = start + gram;
            if end > chars.len() {
                break;
            }

            let start_byte = chars[start].0;
            let end_byte = if end == chars.len() {
                token.text.len()
            } else {
                chars[end].0
            };
            tokens.push(segment_token(token, start_byte, end_byte));
        }
    }

    if tokens.is_empty() {
        vec![token.clone()]
    } else {
        tokens
    }
}

fn split_token_segments(text: &str) -> Vec<TokenSegment> {
    let mut segments = Vec::new();
    let mut current_kind = None;
    let mut current_start = 0;

    for (offset, ch) in text.char_indices() {
        let kind = if is_cjk_bigram_char(ch) {
            SegmentKind::Cjk
        } else {
            SegmentKind::Other
        };

        if let Some(existing) = current_kind {
            if existing != kind {
                segments.push(TokenSegment {
                    kind: existing,
                    start: current_start,
                    end: offset,
                });
                current_start = offset;
            }
        } else {
            current_start = offset;
        }

        current_kind = Some(kind);
    }

    if let Some(kind) = current_kind {
        segments.push(TokenSegment {
            kind,
            start: current_start,
            end: text.len(),
        });
    }

    segments
}

fn cjk_bigram_segment_tokens(token: &Token, segment: &TokenSegment) -> Vec<Token> {
    let segment_text = &token.text[segment.start..segment.end];
    let chars = segment_text.char_indices().collect::<Vec<_>>();
    if chars.len() <= 1 {
        return vec![segment_token(token, segment.start, segment.end)];
    }

    (0..chars.len() - 1)
        .map(|index| {
            let start = segment.start + chars[index].0;
            let end = if index + 2 >= chars.len() {
                segment.end
            } else {
                segment.start + chars[index + 2].0
            };
            segment_token(token, start, end)
        })
        .collect()
}

fn segment_token(token: &Token, start: usize, end: usize) -> Token {
    let mut output = token.clone();
    output.text = token.text[start..end].to_string();
    output.offset_from = token.offset_from + start;
    output.offset_to = token.offset_from + end;
    output
}

fn is_cjk_bigram_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{3040}'..='\u{309F}'
            | '\u{30A0}'..='\u{30FF}'
            | '\u{31F0}'..='\u{31FF}'
            | '\u{AC00}'..='\u{D7AF}'
    )
}

pub fn normalize_multilingual_width(text: &str) -> Cow<'_, str> {
    let mut normalized = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut changed = false;

    while let Some(ch) = chars.next() {
        if ch == '\u{3000}' {
            normalized.push(' ');
            changed = true;
            continue;
        }

        if ('\u{FF01}'..='\u{FF5E}').contains(&ch) {
            normalized.push(
                char::from_u32(ch as u32 - 0xFEE0)
                    .expect("fullwidth ASCII variants should always map to ASCII"),
            );
            changed = true;
            continue;
        }

        if let Some(mapped) = normalize_halfwidth_katakana_char(ch, chars.peek().copied()) {
            normalized.push(mapped);
            if consumes_halfwidth_katakana_mark(ch, chars.peek().copied()) {
                chars.next();
            }
            changed = true;
            continue;
        }

        normalized.push(ch);
    }

    if changed {
        Cow::Owned(normalized)
    } else {
        Cow::Borrowed(text)
    }
}

#[derive(Clone)]
struct MultilingualWidthNormalizer;

impl TokenFilter for MultilingualWidthNormalizer {
    type Tokenizer<T: Tokenizer> = MultilingualWidthNormalizerFilter<T>;

    fn transform<T: Tokenizer>(self, tokenizer: T) -> Self::Tokenizer<T> {
        MultilingualWidthNormalizerFilter {
            tokenizer,
            buffer: String::new(),
        }
    }
}

#[derive(Clone)]
struct MultilingualWidthNormalizerFilter<T> {
    tokenizer: T,
    buffer: String,
}

impl<T: Tokenizer> Tokenizer for MultilingualWidthNormalizerFilter<T> {
    type TokenStream<'a> = MultilingualWidthNormalizerTokenStream<'a, T::TokenStream<'a>>;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        self.buffer.clear();
        MultilingualWidthNormalizerTokenStream {
            tail: self.tokenizer.token_stream(text),
            buffer: &mut self.buffer,
        }
    }
}

struct MultilingualWidthNormalizerTokenStream<'a, T> {
    buffer: &'a mut String,
    tail: T,
}

impl<T: TokenStream> TokenStream for MultilingualWidthNormalizerTokenStream<'_, T> {
    fn advance(&mut self) -> bool {
        if !self.tail.advance() {
            return false;
        }

        if let Cow::Owned(normalized) = normalize_multilingual_width(&self.tail.token().text) {
            self.buffer.clear();
            self.buffer.push_str(&normalized);
            mem::swap(&mut self.tail.token_mut().text, self.buffer);
        }

        true
    }

    fn token(&self) -> &Token {
        self.tail.token()
    }

    fn token_mut(&mut self) -> &mut Token {
        self.tail.token_mut()
    }
}

fn consumes_halfwidth_katakana_mark(base: char, mark: Option<char>) -> bool {
    matches!(mark, Some('ﾞ') if matches!(base, 'ｳ' | 'ｶ' | 'ｷ' | 'ｸ' | 'ｹ' | 'ｺ' | 'ｻ' | 'ｼ' | 'ｽ' | 'ｾ' | 'ｿ' | 'ﾀ' | 'ﾁ' | 'ﾂ' | 'ﾃ' | 'ﾄ' | 'ﾊ' | 'ﾋ' | 'ﾌ' | 'ﾍ' | 'ﾎ'))
        || matches!(mark, Some('ﾟ') if matches!(base, 'ﾊ' | 'ﾋ' | 'ﾌ' | 'ﾍ' | 'ﾎ'))
}

fn normalize_halfwidth_katakana_char(ch: char, mark: Option<char>) -> Option<char> {
    Some(match (ch, mark) {
        ('｡', _) => '。',
        ('｢', _) => '「',
        ('｣', _) => '」',
        ('､', _) => '、',
        ('･', _) => '・',
        ('ｦ', _) => 'ヲ',
        ('ｧ', _) => 'ァ',
        ('ｨ', _) => 'ィ',
        ('ｩ', _) => 'ゥ',
        ('ｪ', _) => 'ェ',
        ('ｫ', _) => 'ォ',
        ('ｬ', _) => 'ャ',
        ('ｭ', _) => 'ュ',
        ('ｮ', _) => 'ョ',
        ('ｯ', _) => 'ッ',
        ('ｰ', _) => 'ー',
        ('ｱ', _) => 'ア',
        ('ｲ', _) => 'イ',
        ('ｳ', Some('ﾞ')) => 'ヴ',
        ('ｳ', _) => 'ウ',
        ('ｴ', _) => 'エ',
        ('ｵ', _) => 'オ',
        ('ｶ', Some('ﾞ')) => 'ガ',
        ('ｶ', _) => 'カ',
        ('ｷ', Some('ﾞ')) => 'ギ',
        ('ｷ', _) => 'キ',
        ('ｸ', Some('ﾞ')) => 'グ',
        ('ｸ', _) => 'ク',
        ('ｹ', Some('ﾞ')) => 'ゲ',
        ('ｹ', _) => 'ケ',
        ('ｺ', Some('ﾞ')) => 'ゴ',
        ('ｺ', _) => 'コ',
        ('ｻ', Some('ﾞ')) => 'ザ',
        ('ｻ', _) => 'サ',
        ('ｼ', Some('ﾞ')) => 'ジ',
        ('ｼ', _) => 'シ',
        ('ｽ', Some('ﾞ')) => 'ズ',
        ('ｽ', _) => 'ス',
        ('ｾ', Some('ﾞ')) => 'ゼ',
        ('ｾ', _) => 'セ',
        ('ｿ', Some('ﾞ')) => 'ゾ',
        ('ｿ', _) => 'ソ',
        ('ﾀ', Some('ﾞ')) => 'ダ',
        ('ﾀ', _) => 'タ',
        ('ﾁ', Some('ﾞ')) => 'ヂ',
        ('ﾁ', _) => 'チ',
        ('ﾂ', Some('ﾞ')) => 'ヅ',
        ('ﾂ', _) => 'ツ',
        ('ﾃ', Some('ﾞ')) => 'デ',
        ('ﾃ', _) => 'テ',
        ('ﾄ', Some('ﾞ')) => 'ド',
        ('ﾄ', _) => 'ト',
        ('ﾅ', _) => 'ナ',
        ('ﾆ', _) => 'ニ',
        ('ﾇ', _) => 'ヌ',
        ('ﾈ', _) => 'ネ',
        ('ﾉ', _) => 'ノ',
        ('ﾊ', Some('ﾞ')) => 'バ',
        ('ﾊ', Some('ﾟ')) => 'パ',
        ('ﾊ', _) => 'ハ',
        ('ﾋ', Some('ﾞ')) => 'ビ',
        ('ﾋ', Some('ﾟ')) => 'ピ',
        ('ﾋ', _) => 'ヒ',
        ('ﾌ', Some('ﾞ')) => 'ブ',
        ('ﾌ', Some('ﾟ')) => 'プ',
        ('ﾌ', _) => 'フ',
        ('ﾍ', Some('ﾞ')) => 'ベ',
        ('ﾍ', Some('ﾟ')) => 'ペ',
        ('ﾍ', _) => 'ヘ',
        ('ﾎ', Some('ﾞ')) => 'ボ',
        ('ﾎ', Some('ﾟ')) => 'ポ',
        ('ﾎ', _) => 'ホ',
        ('ﾏ', _) => 'マ',
        ('ﾐ', _) => 'ミ',
        ('ﾑ', _) => 'ム',
        ('ﾒ', _) => 'メ',
        ('ﾓ', _) => 'モ',
        ('ﾔ', _) => 'ヤ',
        ('ﾕ', _) => 'ユ',
        ('ﾖ', _) => 'ヨ',
        ('ﾗ', _) => 'ラ',
        ('ﾘ', _) => 'リ',
        ('ﾙ', _) => 'ル',
        ('ﾚ', _) => 'レ',
        ('ﾛ', _) => 'ロ',
        ('ﾜ', _) => 'ワ',
        ('ﾝ', _) => 'ン',
        ('ﾞ', _) => '゛',
        ('ﾟ', _) => '゜',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{SearchFieldClass, build_index_time_analyzer, build_query_time_analyzer};
    use tantivy::tokenizer::TokenStream;

    #[test]
    fn multilingual_query_analyzer_lowercases_english_text() {
        assert_multilingual_query_tokens(
            "The incredible adventures of Batman, the man who is also a bat!",
            &[
                "the",
                "incredible",
                "adventures",
                "of",
                "batman",
                "the",
                "man",
                "who",
                "is",
                "also",
                "a",
                "bat",
            ],
        );
    }

    #[test]
    fn multilingual_query_analyzer_folds_accents_to_ascii() {
        assert_multilingual_query_tokens("Éric èl rojo", &["eric", "el", "rojo"]);
    }

    #[test]
    fn multilingual_query_analyzer_keeps_numeric_isbn_tokens_intact() {
        assert_multilingual_query_tokens("9782413016878", &["9782413016878"]);
    }

    #[test]
    fn multilingual_query_analyzer_preserves_tokens_longer_than_forty_chars() {
        assert_multilingual_query_tokens(
            "supercalifragilisticexpialidociousencyclopedia",
            &["supercalifragilisticexpialidociousencyclopedia"],
        );
    }

    #[test]
    fn multilingual_query_analyzer_normalizes_fullwidth_latin_digits_and_spacing() {
        assert_multilingual_query_tokens("Ｂａｔｍａｎ　東京　１２３", &["batman", "東京", "123"]);
    }

    #[test]
    fn multilingual_query_analyzer_treats_fullwidth_punctuation_as_boundaries() {
        assert_multilingual_query_tokens("Hero（東京）１２３", &["hero", "東京", "123"]);
    }

    #[test]
    fn multilingual_query_analyzer_normalizes_halfwidth_katakana_in_mixed_script_text() {
        assert_multilingual_query_tokens("ｶﾀｶﾅ Hero", &["カタ", "タカ", "カナ", "hero"]);
    }

    #[test]
    fn multilingual_query_analyzer_preserves_single_letter_tokens() {
        assert_multilingual_query_tokens("J", &["j"]);
    }

    #[test]
    fn multilingual_query_analyzer_bigrams_chinese_text() {
        assert_multilingual_query_tokens(
            "不道德公會河添太一東立搬运",
            &[
                "不道", "道德", "德公", "公會", "會河", "河添", "添太", "太一", "一東", "東立",
                "立搬", "搬运",
            ],
        );
    }

    #[test]
    fn multilingual_query_analyzer_bigrams_hiragana_mixed_text() {
        assert_multilingual_query_tokens(
            "探偵はもう、死んでいる。",
            &[
                "探偵", "偵は", "はも", "もう", "死ん", "んで", "でい", "いる",
            ],
        );
    }

    #[test]
    fn multilingual_query_analyzer_bigrams_katakana_text() {
        assert_multilingual_query_tokens("ワンパンマン", &["ワン", "ンパ", "パン", "ンマ", "マン"]);
    }

    #[test]
    fn multilingual_query_analyzer_bigrams_korean_text() {
        assert_multilingual_query_tokens(
            "고교생을 환불해 주세요",
            &["고교", "교생", "생을", "환불", "불해", "주세", "세요"],
        );
    }

    #[test]
    fn multilingual_index_analyzer_adds_ngram_approximations_without_touching_exact_fields() {
        assert_eq!(
            collect_tokens(
                build_index_time_analyzer(SearchFieldClass::MultilingualFullText),
                "Batman 東京",
            ),
            vec![
                "batman".to_string(),
                "bat".to_string(),
                "batm".to_string(),
                "batma".to_string(),
                "batman".to_string(),
                "atm".to_string(),
                "atma".to_string(),
                "atman".to_string(),
                "tma".to_string(),
                "tman".to_string(),
                "man".to_string(),
                "東京".to_string(),
            ],
            "index analyzer should keep the original multilingual terms while approximating legacy ngram indexing",
        );
    }

    #[test]
    fn multilingual_index_analyzer_emits_nine_and_ten_char_ngrams() {
        let tokens = collect_tokens(
            build_index_time_analyzer(SearchFieldClass::MultilingualFullText),
            "Encyclopedia",
        );

        assert!(tokens.contains(&"cyclopedi".to_string()));
        assert!(tokens.contains(&"cyclopedia".to_string()));
    }

    #[test]
    fn multilingual_index_analyzer_preserves_tokens_longer_than_forty_chars() {
        let tokens = collect_tokens(
            build_index_time_analyzer(SearchFieldClass::MultilingualFullText),
            "supercalifragilisticexpialidociousencyclopedia",
        );

        assert!(tokens.contains(&"supercalifragilisticexpialidociousencyclopedia".to_string()));
        assert!(tokens.contains(&"alidocious".to_string()));
    }

    #[test]
    fn multilingual_exact_term_analyzers_keep_hyphenated_isbn_as_one_token() {
        for analyzer in [
            build_index_time_analyzer(SearchFieldClass::ExactTerm),
            build_query_time_analyzer(SearchFieldClass::ExactTerm),
        ] {
            assert_eq!(
                collect_tokens(analyzer, "978-1-23"),
                vec!["978-1-23".to_string()],
                "exact-term analyzers must stay keyword-safe while multilingual analyzers evolve",
            );
        }
    }

    fn assert_multilingual_query_tokens(text: &str, expected: &[&str]) {
        let expected = expected
            .iter()
            .map(|token| token.to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            collect_tokens(
                build_query_time_analyzer(SearchFieldClass::MultilingualFullText),
                text,
            ),
            expected,
            "query-side multilingual analyzer should stay close to the legacy search analyzer semantics",
        );
    }

    fn collect_tokens(mut analyzer: tantivy::tokenizer::TextAnalyzer, text: &str) -> Vec<String> {
        let mut stream = analyzer.token_stream(text);
        let mut tokens = Vec::new();

        while stream.advance() {
            tokens.push(stream.token().text.to_string());
        }

        tokens
    }
}
