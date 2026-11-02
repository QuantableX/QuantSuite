"""A tiny stdio MCP server for QuantPilot's live elicitation tests (parity 05).

Tools:
- configure_deploy: asks the client for a form (enum with labels, bounded
  integer, optional text) and returns what came back.
- connect_account: asks the client to finish a step on a web page (URL mode)
  and returns the outcome.

Every result is `ELICIT <tool> action=<action> content=<json>`. The client's
declared capabilities are appended to the file named by QP_ELICIT_LOG.
Standard library only.
"""
import json
import os
import sys

LOG = os.environ.get("QP_ELICIT_LOG")


def log(entry):
    if LOG:
        with open(LOG, "a", encoding="utf-8") as f:
            f.write(json.dumps(entry) + "\n")


def send(msg):
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()


def read():
    while True:
        line = sys.stdin.readline()
        if not line:
            sys.exit(0)
        line = line.strip()
        if line:
            return json.loads(line)


FORM = {
    "type": "object",
    "properties": {
        "target": {"type": "string", "title": "Target", "enum": ["staging", "prod"], "enumNames": ["Staging", "Production"]},
        "replicas": {"type": "integer", "title": "Replicas", "minimum": 1, "maximum": 5},
        "note": {"type": "string", "title": "Note", "maxLength": 40},
    },
    "required": ["target", "replicas"],
}

TOOLS = [
    {"name": "configure_deploy", "description": "Ask the user how to deploy.", "inputSchema": {"type": "object", "properties": {}}},
    {"name": "connect_account", "description": "Ask the user to sign in on a web page.", "inputSchema": {"type": "object", "properties": {}}},
]

pending = []  # client messages read while waiting for an elicitation answer
counter = [0]


def ask(params):
    counter[0] += 1
    rid = f"qa-elicit-{counter[0]}"
    send({"jsonrpc": "2.0", "id": rid, "method": "elicitation/create", "params": params})
    while True:
        msg = read()
        if msg.get("id") == rid and ("result" in msg or "error" in msg):
            log({"elicitation": params.get("mode", "form"), "reply": msg})
            return msg
        pending.append(msg)


def handle(msg):
    method = msg.get("method")
    mid = msg.get("id")
    if method == "initialize":
        log({"client": msg["params"].get("clientInfo"), "capabilities": msg["params"].get("capabilities")})
        send({"jsonrpc": "2.0", "id": mid, "result": {
            "protocolVersion": msg["params"].get("protocolVersion", "2025-06-18"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "qa-elicit", "version": "1.0.0"},
        }})
    elif method == "tools/list":
        send({"jsonrpc": "2.0", "id": mid, "result": {"tools": TOOLS}})
    elif method == "tools/call":
        name = msg["params"]["name"]
        if name == "configure_deploy":
            reply = ask({"mode": "form", "message": "Configure the deploy", "requestedSchema": FORM})
        else:
            reply = ask({"mode": "url", "message": "Sign in to Example to connect your account",
                         "url": "https://example.com/oauth/authorize?state=qp-parity-05", "elicitationId": "qp-el-1"})
            if reply.get("result", {}).get("action") == "accept":
                send({"jsonrpc": "2.0", "method": "notifications/elicitation/complete", "params": {"elicitationId": "qp-el-1"}})
        result = reply.get("result") or {"action": "error", "content": reply.get("error")}
        text = f"ELICIT {name} action={result.get('action')} content={json.dumps(result.get('content'), sort_keys=True)}"
        send({"jsonrpc": "2.0", "id": mid, "result": {"content": [{"type": "text", "text": text}]}})
    elif mid is not None:
        send({"jsonrpc": "2.0", "id": mid, "result": {}})


while True:
    handle(pending.pop(0) if pending else read())
