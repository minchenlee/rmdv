# Math

[← back to README](../../../README.md)

rmdv renders **block math** in `$$…$$` fences natively with `iced_math` — zero JavaScript, no KaTeX.

## The quadratic formula

$$x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}$$

## Euler's identity

$$e^{i\pi} + 1 = 0$$

## A sum

$$\sum_{k=1}^{n} k = \frac{n(n+1)}{2}$$

## A matrix

$$\begin{pmatrix} a & b \\ c & d \end{pmatrix} \begin{pmatrix} x \\ y \end{pmatrix} = \begin{pmatrix} ax + by \\ cx + dy \end{pmatrix}$$

## Binomial

$$\binom{n}{k} = \frac{n!}{k!\,(n-k)!}$$

## Blackboard & calligraphic

$$\mathbb{R} \subset \mathbb{C}, \quad \mathcal{L}(f) = \int_0^\infty f(t)\,e^{-st}\,dt$$

> Note: *inline* `$…$` is shown as literal text — only block `$$…$$` is rendered.

For a whole LaTeX document, see [relativity.tex](../../../papers/research/relativity.tex).
