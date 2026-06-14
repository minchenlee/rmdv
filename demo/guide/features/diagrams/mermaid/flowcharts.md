# Mermaid diagrams

[← back to README](../../../../README.md)

rmdv renders ```mermaid fences natively (no browser).

## Flowchart

```mermaid
flowchart TD
    A[Open folder] --> B{Markdown file?}
    B -->|yes| C[Render]
    B -->|no| D{Data file?}
    D -->|json/yaml| E[Data mind map]
    D -->|else| F[Plain text]
    C --> G[⌘M → mind map]
```

## Sequence

```mermaid
sequenceDiagram
    participant U as User
    participant R as rmdv
    participant FS as Filesystem
    U->>R: ⌘O folder
    R->>FS: walk tree (depth ≤ 8)
    FS-->>R: file list
    R-->>U: sidebar + breadcrumb
```

## State

```mermaid
stateDiagram-v2
    [*] --> Viewing
    Viewing --> Editing: ⌘E
    Editing --> Viewing: ⌘S / Esc
    Viewing --> MindMap: ⌘M
    MindMap --> Viewing: ⌘M
```

See also: [Graphviz DOT →](../graphviz/dot.md)
