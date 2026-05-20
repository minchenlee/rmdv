# Diagram Stress Test

Complex architecture diagrams to exercise the renderer under realistic load.

## 1. System Architecture (Mermaid flowchart)

```mermaid
flowchart TB
    subgraph Client["Client Tier"]
        Web[Web App]
        Mobile[Mobile App]
        CLI[CLI Tool]
    end

    subgraph Edge["Edge / CDN"]
        CDN[CloudFront]
        WAF[Web Firewall]
    end

    subgraph API["API Gateway"]
        GW[Kong Gateway]
        Auth[Auth Service]
        RL[Rate Limiter]
    end

    subgraph Services["Microservices"]
        Users[User Service]
        Orders[Order Service]
        Payments[Payment Service]
        Inventory[Inventory Service]
        Notify[Notification Service]
    end

    subgraph Data["Data Tier"]
        PG[(PostgreSQL)]
        Redis[(Redis Cache)]
        S3[(S3 Storage)]
        Kafka[Kafka Bus]
    end

    subgraph External["External"]
        Stripe[Stripe API]
        SES[AWS SES]
        Twilio[Twilio SMS]
    end

    Web --> CDN
    Mobile --> CDN
    CLI --> WAF
    CDN --> WAF
    WAF --> GW
    GW --> Auth
    GW --> RL
    RL --> Users
    RL --> Orders
    RL --> Payments
    RL --> Inventory

    Users --> PG
    Users --> Redis
    Orders --> PG
    Orders --> Kafka
    Payments --> Stripe
    Payments --> Kafka
    Inventory --> PG
    Inventory --> Redis

    Kafka --> Notify
    Notify --> SES
    Notify --> Twilio
    Orders --> S3
```

## 2. Request Sequence (Mermaid sequence)

```mermaid
sequenceDiagram
    autonumber
    participant U as User
    participant W as Web App
    participant G as API Gateway
    participant A as Auth Service
    participant O as Order Service
    participant P as Payment Service
    participant S as Stripe
    participant K as Kafka
    participant N as Notification

    U->>W: Click "Buy"
    W->>G: POST /orders (JWT)
    G->>A: Validate JWT
    A-->>G: 200 OK + claims
    G->>O: CreateOrder(items, userId)
    O->>O: Reserve inventory
    O->>P: Charge(amount, token)
    P->>S: POST /charges
    S-->>P: 200 + charge_id
    P-->>O: paid
    O->>K: publish OrderCreated
    O-->>G: 201 + order_id
    G-->>W: 201 + order_id
    W-->>U: Show confirmation
    K->>N: consume OrderCreated
    N->>U: Email receipt
```

## 3. Build Pipeline (DOT)

```dot
digraph BuildPipeline {
    rankdir=LR;
    node [shape=box style=rounded];

    Commit -> Lint;
    Commit -> TypeCheck;
    Commit -> UnitTest;

    Lint -> Bundle;
    TypeCheck -> Bundle;
    UnitTest -> Bundle;

    Bundle -> IntegrationTest;
    IntegrationTest -> SecurityScan;
    SecurityScan -> Sign;
    Sign -> StagingDeploy;
    StagingDeploy -> SmokeTest;
    SmokeTest -> CanaryDeploy;
    CanaryDeploy -> ProdDeploy;
    ProdDeploy -> Monitor;
    Monitor -> Rollback [style=dashed label="on alert"];
    Rollback -> StagingDeploy [style=dashed];
}
```

## 4. State Machine (Mermaid stateDiagram)

```mermaid
stateDiagram-v2
    [*] --> Draft
    Draft --> Pending: submit
    Pending --> Approved: reviewer approves
    Pending --> Rejected: reviewer rejects
    Rejected --> Draft: revise
    Approved --> Published: publish
    Published --> Archived: archive
    Archived --> [*]

    Published --> Updated: edit
    Updated --> Published: save
```

## 5. Class / Domain Model (Mermaid classDiagram)

```mermaid
classDiagram
    class User {
        +UUID id
        +String email
        +String name
        +DateTime createdAt
        +login()
        +logout()
    }
    class Order {
        +UUID id
        +UUID userId
        +Money total
        +OrderStatus status
        +place()
        +cancel()
    }
    class LineItem {
        +UUID id
        +UUID orderId
        +UUID productId
        +int quantity
        +Money price
    }
    class Product {
        +UUID id
        +String sku
        +String name
        +Money price
        +int stock
    }
    class Payment {
        +UUID id
        +UUID orderId
        +Money amount
        +PaymentStatus status
        +String stripeId
    }

    User "1" --> "0..*" Order
    Order "1" --> "1..*" LineItem
    LineItem "*" --> "1" Product
    Order "1" --> "0..1" Payment
```

## 6. ER Diagram (Mermaid erDiagram)

```mermaid
erDiagram
    USER ||--o{ ORDER : places
    USER {
        uuid id PK
        string email UK
        string name
        timestamp created_at
    }
    ORDER ||--|{ LINE_ITEM : contains
    ORDER {
        uuid id PK
        uuid user_id FK
        numeric total
        string status
        timestamp created_at
    }
    LINE_ITEM }o--|| PRODUCT : references
    LINE_ITEM {
        uuid id PK
        uuid order_id FK
        uuid product_id FK
        int quantity
        numeric price
    }
    PRODUCT {
        uuid id PK
        string sku UK
        string name
        numeric price
        int stock
    }
    ORDER ||--o| PAYMENT : settles
    PAYMENT {
        uuid id PK
        uuid order_id FK
        numeric amount
        string status
        string stripe_id
    }
```

## 7. Complex DOT (clustered)

```dot
digraph Microservices {
    rankdir=TB;
    compound=true;
    node [shape=box style="rounded,filled"];

    subgraph cluster_edge {
        label="Edge";
        style=dashed;
        CDN; WAF;
    }

    subgraph cluster_app {
        label="Application";
        style=dashed;
        Gateway; Auth; UserSvc; OrderSvc; PaySvc;
    }

    subgraph cluster_data {
        label="Data";
        style=dashed;
        Postgres; Redis; Kafka;
    }

    Client -> CDN;
    CDN -> WAF;
    WAF -> Gateway;
    Gateway -> Auth;
    Gateway -> UserSvc;
    Gateway -> OrderSvc;
    OrderSvc -> PaySvc;

    UserSvc -> Postgres;
    UserSvc -> Redis;
    OrderSvc -> Postgres;
    OrderSvc -> Kafka;
    PaySvc -> Kafka;
}
```

## 8. Broken mermaid (should show error chip)

```mermaid
this is not valid mermaid syntax %%% &&& @@@
```
