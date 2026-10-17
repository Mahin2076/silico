"""Fixed OpenShell verifier bridge. No credentials, shell, or repair execution."""
import json
import sys
import urllib.error
import urllib.request


def fail(code, status=None):
    result = {"p2_bridge_error": code}
    if status is not None:
        result["status"] = status
    print(json.dumps(result))
    raise SystemExit(1)


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, message, headers, newurl):
        return None


try:
    data = sys.stdin.buffer.read(262145)
    if len(data) > 262144:
        fail("invalid_input")
    payload = json.loads(data)
    if not isinstance(payload, dict):
        fail("invalid_input")
    request = urllib.request.Request(
        "https://inference.local/v1/chat/completions",
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.build_opener(NoRedirect).open(request, timeout=25) as response:
        output = response.read(65537)
    if len(output) > 65536:
        fail("too_large")
    sys.stdout.buffer.write(output)
except urllib.error.HTTPError as error:
    fail("http", error.code)
except TimeoutError:
    fail("timeout")
except urllib.error.URLError as error:
    fail("timeout" if isinstance(error.reason, TimeoutError) else "network")
except (ValueError, TypeError):
    fail("invalid_input")
except Exception:
    fail("network")
