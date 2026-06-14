# OAuth 2.0 — `/v2/auth`

[← back to README](../../../../README.md) · [Users endpoint →](../endpoints/users.md)

Deeply nested on purpose: `reference/api/v2/auth/` — watch the breadcrumb and sidebar.

## Token exchange

`POST /v2/auth/token`

| Field | Type | Notes |
|-------|------|-------|
| `grant_type` | string | `authorization_code` or `refresh_token` |
| `code` | string | from the redirect |
| `client_id` | string | your app id |

## Rust client

```rust
#[derive(serde::Serialize)]
struct TokenRequest<'a> {
    grant_type: &'a str,
    code: &'a str,
    client_id: &'a str,
}

async fn exchange(code: &str, id: &str) -> reqwest::Result<String> {
    let body = TokenRequest { grant_type: "authorization_code", code, client_id: id };
    let resp = reqwest::Client::new()
        .post("https://api.example.com/v2/auth/token")
        .json(&body)
        .send()
        .await?;
    resp.text().await
}
```

## Java client

```java
var req = HttpRequest.newBuilder()
    .uri(URI.create("https://api.example.com/v2/auth/token"))
    .header("Content-Type", "application/json")
    .POST(BodyPublishers.ofString(payload))
    .build();
```

## Audit query

```sql
SELECT client_id, COUNT(*) AS token_grants
FROM auth_events
WHERE event = 'token_issued' AND ts > NOW() - INTERVAL '1 day'
GROUP BY client_id
ORDER BY token_grants DESC;
```
