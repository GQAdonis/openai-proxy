"""UI-1 and SURFACE-1 subcases for the single authorized boundary invocation."""
from hashlib import sha256
from html.parser import HTMLParser
from urllib.parse import urlsplit

from admin_common import case, require
import ui_browser
import ui_build
import ui_upstream


class Index(HTMLParser):
    def __init__(self):
        super().__init__()
        self.assets = {}

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if tag == "script" and "src" in a:
            self.assets[a["src"]] = "javascript"
        if tag == "link" and "href" in a:
            if a.get("rel") == "stylesheet":
                self.assets[a["href"]] = "text/css"
            elif a.get("rel") == "icon":
                self.assets[a["href"]] = "image/"


def assets(h, base, actor):
    refs = Index()
    digests = {}
    for path in ("/ui/", "/ui/settings"):
        status, heads, data = h.http(path, headers={"Accept": "text/html"}, base=base, actor=actor)
        require(status == 200 and heads.get("content-type", "").startswith("text/html"), "missing console index")
        require("no-cache" in heads.get("cache-control", "") or "no-store" in heads.get("cache-control", ""),
                "index is inappropriately cacheable")
        digests[path] = sha256(data).hexdigest()
        refs.feed(data.decode())
    require(digests["/ui/"] == digests["/ui/settings"], "settings deep link did not serve packaged entry")
    require({"javascript", "text/css"} <= set(refs.assets.values()), "missing JS or CSS in embedded entry")
    for path, mime in refs.assets.items():
        require(path.startswith("/ui/") and not urlsplit(path).netloc, "console depends on an external resource")
        status, heads, data = h.http(path, base=base, actor=actor)
        require(status == 200 and data and mime in heads.get("content-type", ""), "asset missing or incorrect media type")
        if path.startswith("/ui/assets/"):
            require("immutable" in heads.get("cache-control","") and "max-age=" in heads["cache-control"],
                    "content-addressed asset lacks immutable caching")
        digests[path] = sha256(data).hexdigest()
    return digests


def missing(h, base, actor):
    for path in ("/api/v1/kbd-missing", "/ui/assets/kbd-missing.js"):
        code, headers, data = h.http(path, headers={"Accept": "text/html"}, base=base, actor=actor)
        require(code == 404 and b'<div id="root"' not in data, "missing API or asset incorrectly served application")
    return {"statuses": [404, 404]}


def rejected_settings(h):
    path = h.root / "shared/state/defaults.json"
    before = path.read_bytes() if path.exists() else None
    status, _, data = h.http("/api/v1/admin/settings", method="PATCH", body={"defaultModel": "kbd-ui-refused"},
                              base=h.shared, actor="chat")
    require(status == 403, "chat user's host mutation did not return forbidden")
    require(all(k.encode() not in data for k in h.keys.values()) and str(h.root).encode() not in data,
            "rejection disclosed fixture credentials or service paths")
    require((path.read_bytes() if path.exists() else None) == before, "host defaults changed after rejection")
    return {"status": status, "changed": False}


def run(h, row, negative):
    if row not in ("UI-1", "SURFACE-1"):
        return False
    if row == "SURFACE-1":
        if negative:
            for kind in ("text", *ui_upstream.BAD):
                case(h, "invalid surface " + kind, lambda kind=kind: ui_browser.invalid_wire(h, kind))
        case(h, "two-owner surface boundaries" if negative else "typed projection, keyboard action and replay",
             lambda: ui_browser.action_journey(h, negative))
        return True
    for actor, base in (("local", h.base), ("chat", h.shared), ("image", h.image_base)):
        label = "final image" if actor == "image" else actor
        if not base:
            h.block(label + " packaged console", "final image service unavailable")
            continue
        if negative:
            case(h, label + " asset and API errors", lambda: missing(h, base, actor))
        else:
            case(h, label + " packaged asset and cache", lambda: assets(h, base, actor))
            case(h, label + " keyboard settings and driver capabilities", lambda: ui_browser.nav(h, base, actor))
            case(h, label + " native stop", lambda: ui_browser.model_turn(h, base, actor))
    if not negative:
        case(h, "standalone release without worker runtime", lambda: ui_build.degraded(h))
    if negative:
        case(h, "failed native stream preserves usable text", lambda: ui_browser.model_turn(h, h.shared, "chat", True))
        case(h, "chat user cannot write host settings", lambda: rejected_settings(h))
    ui_build.check(h, negative)
    return True
