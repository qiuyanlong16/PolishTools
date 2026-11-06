import json
from http.server import BaseHTTPRequestHandler, HTTPServer

RESULT = {
    "polished": "This is a test sentence, and I have an apple.",
    "issues": [
        {"type": "grammar", "original": "This are", "corrected": "This is", "explain": "主谓不一致"},
        {"type": "article", "original": "a apple", "corrected": "an apple", "explain": "元音音素开头用 an"},
    ],
    "summary": "修正主谓一致与冠词",
}


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        raw = self.rfile.read(length)
        with open("mock_requests.log", "ab") as f:
            f.write(raw + b"\n")

        content = json.dumps(RESULT, ensure_ascii=False)
        body = json.dumps(
            {"choices": [{"message": {"content": content}}]}, ensure_ascii=False
        ).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


if __name__ == "__main__":
    HTTPServer(("127.0.0.1", 8899), Handler).serve_forever()