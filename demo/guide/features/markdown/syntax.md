# Markdown syntax kitchen sink

[← back to README](../../../README.md)

Everything rmdv's renderer supports, in one page. Press **⌘M** to mind-map it, **⌘E** to edit it.

## Text

**Bold**, *italic*, ***both***, ~~strikethrough~~, `inline code`, and a [link](../../getting-started/first-steps/quickstart.md).

## Headings drive the outline

Use ⌘↑ / ⌘↓ to hop between these. The outline panel mirrors them.

### A third-level heading
#### A fourth-level heading

## Lists

- Top level
  - Nested once
    - Nested twice
      - Nested three deep
- Back to top

1. Ordered
2. Lists
   1. With nested
   2. Numbering

## Task list

- [x] Render headings
- [x] Render tables
- [ ] Render your TODO

## Table

| Language | Paradigm | Year |
|----------|----------|------|
| Rust | Systems / functional | 2010 |
| Python | Multi-paradigm | 1991 |
| SQL | Declarative | 1974 |

## Blockquote

> "Simplicity is prerequisite for reliability."
> — Edsger W. Dijkstra
>
> > Nested quotes work too.

## Code with syntax highlighting

Rust:

```rust
fn main() {
    let nums = vec![1, 2, 3];
    let sum: i32 = nums.iter().sum();
    println!("sum = {sum}");
}
```

Python:

```python
def fib(n: int) -> int:
    a, b = 0, 1
    for _ in range(n):
        a, b = b, a + b
    return a
```

C++:

```cpp
#include <vector>
template <typename T>
T sum(const std::vector<T>& xs) {
    T acc{};
    for (const auto& x : xs) acc += x;
    return acc;
}
```

Java:

```java
public record Point(int x, int y) {
    int manhattan() { return Math.abs(x) + Math.abs(y); }
}
```

SQL:

```sql
SELECT u.name, COUNT(o.id) AS orders
FROM users u
LEFT JOIN orders o ON o.user_id = u.id
GROUP BY u.name
HAVING COUNT(o.id) > 5;
```

TypeScript:

```typescript
const greet = (name: string): string => `hello, ${name}`;
```

## Horizontal rule

---

That's the whole renderer in one file.
