# mermaid-rs-renderer trial

Open this file in rmdv. Try a light and a dark theme, then click a diagram to
check zoom and pan. Labels, arrows, and node borders should remain readable.

## 中文流程圖

```mermaid
flowchart LR
    A[開始] -->|讀取 Markdown| B{包含圖表？}
    B -->|是| C[Rust renderer 產生 SVG]
    B -->|否| D[顯示文字]
    C --> E[完成]
    D --> E
```

## Sequence

```mermaid
sequenceDiagram
    participant Reader
    participant Renderer
    Reader->>Renderer: Render diagram
    Renderer-->>Reader: SVG
```

## Class

```mermaid
classDiagram
    Document "1" --> "*" Diagram
    Document : +String title
    Diagram : +render() SVG
```

## State

```mermaid
stateDiagram-v2
    [*] --> Pending
    Pending --> Ready: rendered
    Pending --> Error: invalid syntax
    Ready --> [*]
```

## Mindmap

```mermaid
mindmap
    root((Project))
        Parse
        Layout
        Render
```

## Explicit theme override

```mermaid
%%{init: {"theme":"base","themeVariables":{"primaryColor":"#d9ead3","primaryTextColor":"#182018","primaryBorderColor":"#38761d"}}}%%
flowchart LR
    A[Custom green] --> B[Preserved theme]
```

## Invalid source

This should show a diagram error without preventing the other diagrams from
rendering.

```mermaid
this is not valid mermaid
```
