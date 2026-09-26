#!/usr/bin/env python3
"""Минимальный S3-совместимый сервер для тестов клиента.

Понимает то же, что использует Muzon: ListObjectsV2 с пагинацией, PUT и GET
объекта. Проверяет, что запрос подписан (заголовки SigV4 на месте) — сама
подпись сверяется отдельным юнит-тестом по эталонному вектору AWS.

Печатает строку "READY <порт>" в stdout, когда готов принимать запросы.
"""

import sys
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import urlparse, parse_qs, unquote

BUCKET = "muzon"
objects: dict[str, bytes] = {}
lock = threading.Lock()


def xml_escape(s: str) -> str:
    return (
        s.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def _key(self) -> str:
        path = unquote(urlparse(self.path).path)
        prefix = f"/{BUCKET}/"
        return path[len(prefix):] if path.startswith(prefix) else ""

    def _signed(self) -> bool:
        auth = self.headers.get("authorization", "")
        return (
            auth.startswith("AWS4-HMAC-SHA256 ")
            and "Signature=" in auth
            and self.headers.get("x-amz-date") is not None
            and self.headers.get("x-amz-content-sha256") is not None
        )

    def _deny(self):
        body = b"<Error><Code>AccessDenied</Code><Message>not signed</Message></Error>"
        self.send_response(403)
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_PUT(self):
        if not self._signed():
            return self._deny()
        length = int(self.headers.get("content-length", 0))
        body = self.rfile.read(length)
        with lock:
            objects[self._key()] = body
        self.send_response(200)
        self.send_header("content-length", "0")
        self.end_headers()

    def do_GET(self):
        if not self._signed():
            return self._deny()

        query = parse_qs(urlparse(self.path).query)
        if query.get("list-type") == ["2"]:
            return self._list(query)

        with lock:
            body = objects.get(self._key())
        if body is None:
            msg = b"<Error><Code>NoSuchKey</Code><Message>gone</Message></Error>"
            self.send_response(404)
            self.send_header("content-length", str(len(msg)))
            self.end_headers()
            self.wfile.write(msg)
            return

        self.send_response(200)
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _list(self, query):
        prefix = query.get("prefix", [""])[0]
        # Пагинация через одну страницу за раз, чтобы тест прошёл её по-настоящему
        page_size = 2
        token = int(query.get("continuation-token", ["0"])[0])

        with lock:
            keys = sorted(k for k in objects if k.startswith(prefix))
        page = keys[token:token + page_size]
        next_token = token + page_size
        truncated = next_token < len(keys)

        items = "".join(
            f"<Contents><Key>{xml_escape(k)}</Key><Size>{len(objects[k])}</Size>"
            f"<ETag>&quot;deadbeef&quot;</ETag></Contents>"
            for k in page
        )
        body = (
            '<?xml version="1.0" encoding="UTF-8"?>'
            "<ListBucketResult>"
            f"<IsTruncated>{'true' if truncated else 'false'}</IsTruncated>"
            f"{items}"
            + (f"<NextContinuationToken>{next_token}</NextContinuationToken>" if truncated else "")
            + "</ListBucketResult>"
        ).encode()

        self.send_response(200)
        self.send_header("content-type", "application/xml")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


if __name__ == "__main__":
    server = HTTPServer(("127.0.0.1", 0), Handler)
    print(f"READY {server.server_port}", flush=True)
    server.serve_forever()
