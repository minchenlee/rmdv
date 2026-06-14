# Users — `/v2/users`

[← back to README](../../../../README.md) · [← OAuth](../auth/oauth.md)

## List users

`GET /v2/users?limit=20&cursor=…`

Response:

```json
{
  "data": [
    { "id": 1, "name": "Ada Lovelace", "role": "admin" },
    { "id": 2, "name": "Alan Turing", "role": "member" }
  ],
  "next_cursor": "eyJpZCI6Mn0="
}
```

## TypeScript SDK

```typescript
interface User { id: number; name: string; role: "admin" | "member"; }

async function listUsers(limit = 20): Promise<User[]> {
  const r = await fetch(`/v2/users?limit=${limit}`);
  const { data } = await r.json();
  return data as User[];
}
```

> The full request/response schema lives in [config.json](../../../data/config.json) — open it and press **⌘M**.
