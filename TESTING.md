# Behind nginx on nix

## Backend

```bash
$ cargo r -p server -- -U <user> -c -vvv -s <PORT1> <location>
```

## Frontend

```bash
$ VITE_URL_HOST='<host>' VITE_URL_TLS='true' VITE_URL_AUTH='<auth_host>' yarn dev --host --port <PORT2>
```

## Nginx

```nginx

nginx = {
      virtualHosts = {
        "<host>" = {
          forceSSL = true;
          enableACME = true;
          http2 = true;
          http3 = true;
          extraConfig = ''
            endgame on;
          '';

          locations = {
            "/" = {
              proxyPass = "http://localhost:<PORT2>/";
              recommendedProxySettings = true;
              proxyWebsockets = true;
            };
            "/api/" = {
              proxyPass = "http://localhost:<PORT1>/api/";
              recommendedProxySettings = true;
            };
            "/ws/binary" = {
              proxyPass = "http://localhost:<PORT1>/ws/binary";
              recommendedProxySettings = true;
              proxyWebsockets = true;
            };
            "/ws/text" = {
              proxyPass = "http://localhost:<PORT1>/ws/text";
              recommendedProxySettings = true;
              proxyWebsockets = true;
            };
          };
        };
    };
}
```

# Locally

## Backend

```bash
$ mkdir -p ./.dev/db
$ cargo r -p server --features dev -- -U <user> -c -vvv -s 3333 ./.dev/db
```

Optionally, insert data

```
$ curl \
  localhost:3333/skull \
  -XPOST \
  -H 'Content-Type: application/json' \
  -d '
{
  "name": "aa",
  "color": 16733440,
  "icon": "fas fa-circle",
  "price": 0
}'
```

## Frontend

```bash
$ yarn dev --host
```
