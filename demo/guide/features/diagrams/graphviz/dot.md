# Graphviz DOT

[← back to README](../../../../README.md)

rmdv renders ```dot (and ```graphviz) fences via a pure-Rust layout engine.

## Dependency graph

```dot
digraph deps {
    rankdir=LR;
    node [shape=box, style=rounded];
    parser -> renderer;
    parser -> outline;
    renderer -> diagram;
    renderer -> math;
    diagram -> mermaid;
    diagram -> dot;
    renderer -> highlight;
}
```

## A simple state machine

```graphviz
digraph fsm {
    rankdir=LR;
    node [shape=circle];
    start -> idle;
    idle -> loading [label="open"];
    loading -> ready [label="parsed"];
    ready -> idle [label="close"];
}
```

Back to [Mermaid ←](../mermaid/flowcharts.md)
