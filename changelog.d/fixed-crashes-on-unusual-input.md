- The desktop client and the phone no longer crash on a tool's label in Chinese, Russian or
  another script whose letters take several bytes.
- The desktop client no longer crashes on a link holding `%` before an accented letter or a
  symbol (`notes/%aé.md`), on an SVG image declaring a huge size (it is refused, as a PNG that
  large is), or on an answer nesting quotes, lists or `<details>` thousands deep (levels past 32
  join their parent).
- An image whose server sends a chunk size larger than the answer no longer crashes the client;
  what came before it is kept.
- The desktop client no longer crashes on an answer's HTML holding `&` before Japanese, Chinese or
  Russian text.
