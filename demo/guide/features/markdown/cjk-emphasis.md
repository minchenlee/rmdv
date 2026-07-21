# CJK emphasis regression fixture

Open this page in `rmdv` and check the rendered result visually. The text
between the markers should be styled; the surrounding CJK text should remain
plain and intact. CJK fallback fonts may stay upright in italic spans because
most platform CJK fonts do not ship italic faces; they must still display, and
Latin text in the same span should remain visibly italic.

## Traditional Chinese

這是**粗體中文（含全形括號）**後面的文字。

前綴**重點。**後綴。

前綴**「引號中的重點」**後綴。

前綴**【方括號中的重點】**後綴。

前綴**《書名號中的重點》**後綴。

前綴*斜體中文。*後綴。

前綴*「引號中的斜體」*後綴。

這是**粗體裡的*斜體*文字**，後面還有正常文字。

## Japanese

これは**重要な内容。**その後も文章が続きます。

前綴**「日本語の強調」**後綴。

前綴*日本語の斜体。*後綴。

## Korean

이 문장은 **한국어 강조** 문장입니다.

앞뒤에**한국어 강조**문장이 이어집니다.

## Mixed scripts and symbols

繁體中文 / 简体中文 / 日本語 / 한국어 / English / ＡＢＣ / 全形１２３ / 😊

這是**CJK + Latin + 123 + 😊 的混合粗體。**後面保持正常。

## Other Markdown combinations

- **列表中的中文重點。**
- *列表中的中文斜體。*
- [**中文連結文字**](https://example.com/cjk)
- `（X）**的` ← code span 裡的星號應該保持原樣，不應變成粗體

| 語言 | 強調 | 後續文字 |
| --- | --- | --- |
| 中文 | **重點。** | 應保持正常 |
| 日本語 | *重要。* | そのまま |
| 한국어 | **강조** | 정상 |

## Must stay literal inside code

```text
（X）**這段在 fenced code 裡不應變粗體**
前綴*這段也應保持原樣*後綴
```

## Pass criteria

1. Every intended `**...**` and `*...*` span above is visibly styled; CJK italic text may remain upright but must display.
2. CJK punctuation such as `。`, `」`, `）`, `】`, and `》` is included in the styled span.
3. Text immediately before and after each span is not swallowed, reordered, or corrupted.
4. The inline-code and fenced-code examples show literal asterisks.
