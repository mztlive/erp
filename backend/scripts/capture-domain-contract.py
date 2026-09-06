#!/usr/bin/env python3
"""Capture HTTP/DTO/index/idempotency/error goldens from production sources.

The output is a comparison baseline for later domain-crate phases. It reads
only in-tree sources and generated permission files; it does not connect to
MongoDB or start web-api. Real database execution is recorded as unverified.
"""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any


PERM_ITEM = re.compile(
    r"method:\s*\"(?P<method>GET|POST|PUT|PATCH|DELETE)\"\s*,\s*"
    r"path:\s*\"(?P<path>[^\"]+)\"\s*,\s*"
    r"description:\s*\"(?P<description>[^\"]+)\"\s*,\s*"
    r"permission:\s*\{\s*"
    r"resource:\s*\"(?P<resource>[^\"]+)\"\s*,\s*"
    r"action:\s*\"(?P<action>[^\"]+)\"",
    re.S,
)
ERROR_VARIANT = re.compile(r"^\s+([A-Z][A-Za-z0-9]+),?\s*$", re.M)
ERROR_AS_STR = re.compile(
    r"Self::([A-Za-z0-9]+)\s*=>\s*\"([A-Z0-9_]+)\"",
)
HTTP_STATUS = re.compile(
    r"Error::([A-Za-z]+)\([^)]*\)\s*=>\s*StatusCode::([A-Z_]+)",
)
HTTP_CODE = re.compile(
    r"Error::([A-Za-z]+)\([^)]*\)\s*=>\s*\"([A-Z0-9_]+)\"",
)
COLLECTION_CONST = re.compile(
    r"const\s+([A-Z][A-Z0-9_]*)\s*:\s*&'static\s+str\s*=\s*\"([^\"]+)\""
)
INDEX_CALL = re.compile(
    r"""(?P<kind>unique_index|named_index)\s*\(\s*"(?P<name>[^"]+)"\s*,\s*doc!\s*\{(?P<keys>.*?)\}""",
    re.S,
)
DTO_STRUCT = re.compile(
    r"pub struct ([A-Za-z0-9]+)\s*\{(.*?)\}",
    re.S,
)
DTO_FIELD = re.compile(
    r"(?:#\[serde\([^\]]*\)\]\s*)*pub(?:\([^\)]+\))?\s+([a-z][A-Za-z0-9_]*)\s*:",
    re.S,
)
SCALE = re.compile(r"const ([A-Z_]+SCALE[A-Z_]*)\s*:\s*u32\s*=\s*(\d+);")


def sha256_file(path: Path) -> str:
    """Return hex SHA-256 of a file's bytes."""
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path: Path) -> str:
    """Read UTF-8 text."""
    return path.read_text(encoding="utf-8")


def capture_http(repo: Path) -> dict[str, Any]:
    """Capture generated permission routes as the HTTP/RBAC golden."""
    generated = repo / "erp-client/lib/permissions.generated.ts"
    text = read(generated)
    routes = []
    for match in PERM_ITEM.finditer(text):
        routes.append(
            {
                "method": match.group("method"),
                "path": match.group("path"),
                "description": match.group("description"),
                "resource": match.group("resource"),
                "action": match.group("action"),
            }
        )
    public_login = {
        "method": "POST",
        "path": "/login",
        "description": "公开登录",
        "resource": None,
        "action": None,
        "source": "apps/web-api/src/core/routes/public.rs",
    }
    upload = {
        "method": "POST",
        "path": "/upload",
        "description": "认证后上传",
        "resource": None,
        "action": None,
        "source": "apps/web-api/src/core/routes/mod.rs",
    }
    return {
        "permissions_generated_sha256": sha256_file(generated),
        "admin_route_count": len(routes),
        "admin_routes": routes,
        "extra_routes": [public_login, upload],
        "admin_prefix": "/admin",
        "admin_auth": "JWT + RBAC with_permission",
    }


def capture_errors(backend: Path) -> dict[str, Any]:
    """Capture stable error codes and HTTP status mapping."""
    services_errors = backend / "services/src/errors.rs"
    http_errors = backend / "apps/web-api/src/core/errors.rs"
    services_text = read(services_errors)
    http_text = read(http_errors)
    enum_block = services_text.split("pub enum ErrorCode {", 1)[1].split("}", 1)[0]
    variants = ERROR_VARIANT.findall(enum_block)
    as_str = dict(ERROR_AS_STR.findall(services_text))
    http_status = dict(HTTP_STATUS.findall(http_text))
    http_code = dict(HTTP_CODE.findall(http_text))
    class_block = ""
    if "pub enum ErrorClass" in services_text:
        class_block = services_text.split("pub enum ErrorClass {", 1)[1].split("}", 1)[0]
    return {
        "services_errors_sha256": sha256_file(services_errors),
        "http_errors_sha256": sha256_file(http_errors),
        "error_class_variants": [line.strip().strip(",") for line in class_block.splitlines() if line.strip().startswith("///") is False and line.strip()],
        "error_code_variants": variants,
        "error_code_as_str": as_str,
        "http_status_by_variant": http_status,
        "http_code_by_variant": http_code,
        "notes": "错误类别不得靠字符串包含判断；HTTP 映射以 as_str/class 为准",
    }


def capture_indexes(backend: Path) -> dict[str, Any]:
    """Capture collection names and named index keys/uniqueness."""
    collections: dict[str, str] = {}
    ext_dir = backend / "database/src/repository/extensions"
    for path in sorted(ext_dir.glob("*.rs")):
        if path.name == "mod.rs":
            continue
        for match in COLLECTION_CONST.finditer(read(path)):
            collections[match.group(1)] = match.group(2)
    indexes: list[dict[str, Any]] = []
    index_dir = backend / "database/src/indexes"
    for path in sorted(index_dir.glob("*.rs")):
        if path.name == "mod.rs":
            continue
        text = read(path)
        for match in INDEX_CALL.finditer(text):
            keys_raw = re.sub(r"\s+", " ", match.group("keys")).strip().strip(",")
            indexes.append(
                {
                    "file": path.relative_to(backend).as_posix(),
                    "name": match.group("name"),
                    "unique": match.group("kind") == "unique_index",
                    "keys": keys_raw,
                }
            )
    return {
        "collection_constants": collections,
        "index_count": len(indexes),
        "indexes": indexes,
        "authority": "repository extensions associated constants + indexes/<domain>.rs",
    }


def capture_money(backend: Path) -> dict[str, Any]:
    """Capture money/qty precision and serde contract."""
    money = backend / "entities/src/money.rs"
    text = read(money)
    scales = dict(SCALE.findall(text))
    return {
        "file": "entities/src/money.rs",
        "sha256": sha256_file(money),
        "types": {
            "Amount": {"scale": 2, "json": "string", "bson": "Decimal128"},
            "UnitPrice": {"scale": 4, "json": "string", "bson": "Decimal128"},
            "Quantity": {"scale": 6, "json": "string", "bson": "Decimal128"},
            "Rate": {"scale": 6, "json": "string", "bson": "Decimal128"},
        },
        "declared_scales": scales,
        "rounding": "round_to_cent bankers rounding; constructors reject excess scale",
        "human_readable_json_string": "human-readable (serde_json) 序列化为字符串" in text
        or "序列化为字符串" in text,
        "bson_decimal128": "bson::Decimal128" in text,
    }


def capture_idempotency(backend: Path) -> dict[str, Any]:
    """Capture command fingerprint and sales/customer idempotency algorithms."""
    command = backend / "entities/src/command.rs"
    sales = backend / "services/src/sales_order/command/identity.rs"
    customer = backend / "services/src/customer/profile/validation.rs"
    command_text = read(command)
    return {
        "command_fingerprint": {
            "file": "entities/src/command.rs",
            "sha256": sha256_file(command),
            "prefix": "sha256-v1:" if "sha256-v1:" in command_text else None,
            "domain_separator": "command-fingerprint-v1" if "command-fingerprint-v1" in command_text else None,
            "encoding": "length-prefixed parts, no Debug/JSON map order",
        },
        "sales_submission_fingerprint": {
            "file": "services/src/sales_order/command/identity.rs",
            "sha256": sha256_file(sales),
            "algorithm": "sha256(serde_json(actor_id, sales_order_id, request)) hex",
        },
        "customer_profile_replay": {
            "file": "services/src/customer/profile/validation.rs",
            "sha256": sha256_file(customer),
        },
    }


def capture_dto(backend: Path) -> dict[str, Any]:
    """Hash DTO sources and list public struct fields for later comparison."""
    files: list[dict[str, Any]] = []
    for path in sorted(backend.rglob("dto.rs")):
        rel = path.relative_to(backend).as_posix()
        if "/tests/" in f"/{rel}/":
            continue
        text = read(path)
        structs = []
        for match in DTO_STRUCT.finditer(text):
            fields = DTO_FIELD.findall(match.group(2))
            structs.append({"name": match.group(1), "fields": fields})
        files.append({"path": rel, "sha256": sha256_file(path), "structs": structs})
    return {"dto_file_count": len(files), "files": files}


def capture_transaction(backend: Path) -> dict[str, Any]:
    """Capture Executor/transaction contract traces from source."""
    executor = backend / "database/src/executor.rs"
    txn = backend / "database/src/transaction.rs"
    audited = backend / "services/src/transaction.rs"
    txn_text = read(txn)
    audited_text = read(audited)
    return {
        "real_database_verified": False,
        "disclaimer": "真实数据库运行未验证",
        "executor": {
            "file": "database/src/executor.rs",
            "sha256": sha256_file(executor),
            "no_transaction_session": "None",
            "client_session_session": "Some(self)",
            "nested_transactions": "forbidden",
        },
        "transactional": {
            "file": "database/src/transaction.rs",
            "sha256": sha256_file(txn),
            "read_concern": "snapshot" if "ReadConcern::snapshot()" in txn_text else None,
            "write_concern": "majority" if "WriteConcern::majority()" in txn_text else None,
            "unknown_commit_timeout_secs": 120 if "COMMIT_RETRY_TIMEOUT" in txn_text else None,
            "failed_session_reuse": "forbidden",
        },
        "run_audited": {
            "file": "services/src/transaction.rs",
            "sha256": sha256_file(audited),
            "order": ["write(db, session)", "audit_logs().create(&audit, session)"],
            "external_io_in_write": "forbidden",
            "same_executor": True,
        },
        "source_contains_with_transaction": "with_transaction" in audited_text,
    }


def main() -> int:
    """Write contract-comparison.json and transaction-contract.json to --output."""
    backend = Path(sys.argv[1]).resolve()
    output = Path(sys.argv[2]).resolve()
    output.mkdir(parents=True, exist_ok=True)
    repo = backend.parent
    comparison = {
        "phase": "00",
        "real_database_verified": False,
        "disclaimer": "真实数据库运行未验证",
        "http": capture_http(repo),
        "errors": capture_errors(backend),
        "indexes": capture_indexes(backend),
        "money": capture_money(backend),
        "idempotency": capture_idempotency(backend),
        "dto": capture_dto(backend),
    }
    transaction = capture_transaction(backend)
    (output / "contract-comparison.json").write_text(
        json.dumps(comparison, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    (output / "transaction-contract.json").write_text(
        json.dumps(transaction, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(
        json.dumps(
            {
                "admin_routes": comparison["http"]["admin_route_count"],
                "error_codes": len(comparison["errors"]["error_code_variants"]),
                "indexes": comparison["indexes"]["index_count"],
                "collections": len(comparison["indexes"]["collection_constants"]),
                "dto_files": comparison["dto"]["dto_file_count"],
            },
            ensure_ascii=False,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
