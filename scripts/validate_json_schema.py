#!/usr/bin/env python3
"""Minimal, dependency-free JSON Schema validator for the BGA contract files.

The repository's CI is stdlib-only.  Rather than install ``jsonschema``, this
module implements the small subset of JSON Schema draft 2020-12 that the two
BGA schemas actually use:

* ``type`` (including unions such as ``["string", "null"]``),
* ``required``, ``properties``, ``additionalProperties: false``,
* ``enum``, ``const``, ``pattern``, ``minLength``, ``minItems``, ``uniqueItems``,
* ``items``, ``oneOf``, ``anyOf``, ``$ref`` into ``$defs``, ``format: date``.

It is intentionally strict about unknown keywords: a keyword it does not know
raises an error so that a schema change cannot silently weaken validation.
"""

from __future__ import annotations

import json
import re
from typing import Any

KNOWN_KEYWORDS = {
    "$schema",
    "$id",
    "title",
    "description",
    "type",
    "required",
    "properties",
    "additionalProperties",
    "enum",
    "const",
    "pattern",
    "minLength",
    "minItems",
    "uniqueItems",
    "items",
    "oneOf",
    "anyOf",
    "$ref",
    "$defs",
    "format",
    "minimum",
    "maximum",
}

TYPES = {
    "object": dict,
    "array": list,
    "string": str,
    "integer": int,
    "number": (int, float),
    "boolean": bool,
    "null": type(None),
}


def _type_ok(value: Any, expected: str) -> bool:
    if expected == "integer":
        return isinstance(value, int) and not isinstance(value, bool)
    if expected == "number":
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    if expected == "boolean":
        return isinstance(value, bool)
    return isinstance(value, TYPES[expected])


def validate(instance: Any, schema: dict, path: str = "$", root: dict | None = None) -> list[str]:
    """Return a list of human-readable errors (empty means valid)."""
    root = root or schema
    errors: list[str] = []
    for keyword in schema:
        if keyword not in KNOWN_KEYWORDS:
            errors.append(f"{path}: unsupported schema keyword {keyword!r}")
    if "$ref" in schema:
        ref = schema["$ref"]
        if not ref.startswith("#/"):
            return [f"{path}: unsupported $ref {ref!r}"]
        target: Any = root
        for part in ref[2:].split("/"):
            target = target[part]
        return validate(instance, target, path, root)
    if "type" in schema:
        expected = schema["type"]
        expected = expected if isinstance(expected, list) else [expected]
        if not any(_type_ok(instance, item) for item in expected):
            errors.append(f"{path}: expected type {expected}, got {type(instance).__name__}")
            return errors
    if "const" in schema and instance != schema["const"]:
        errors.append(f"{path}: expected const {schema['const']!r}")
    if "enum" in schema and instance not in schema["enum"]:
        errors.append(f"{path}: value {instance!r} is not in the enum")
    if isinstance(instance, str):
        if "minLength" in schema and len(instance) < schema["minLength"]:
            errors.append(f"{path}: string shorter than {schema['minLength']}")
        if "pattern" in schema and not re.fullmatch(schema["pattern"], instance):
            errors.append(f"{path}: string does not match {schema['pattern']!r}")
        if schema.get("format") == "date" and not re.fullmatch(r"\d{4}-\d{2}-\d{2}", instance):
            errors.append(f"{path}: not a date")
    if isinstance(instance, int) and not isinstance(instance, bool):
        if "minimum" in schema and instance < schema["minimum"]:
            errors.append(f"{path}: below minimum {schema['minimum']}")
        if "maximum" in schema and instance > schema["maximum"]:
            errors.append(f"{path}: above maximum {schema['maximum']}")
    if isinstance(instance, list):
        if "minItems" in schema and len(instance) < schema["minItems"]:
            errors.append(f"{path}: fewer than {schema['minItems']} items")
        if schema.get("uniqueItems") and len({json.dumps(item, sort_keys=True) for item in instance}) != len(instance):
            errors.append(f"{path}: items are not unique")
        if "items" in schema:
            for index, item in enumerate(instance):
                errors.extend(validate(item, schema["items"], f"{path}[{index}]", root))
    if isinstance(instance, dict):
        for key in schema.get("required", []):
            if key not in instance:
                errors.append(f"{path}: missing required key {key!r}")
        properties = schema.get("properties", {})
        for key, value in instance.items():
            if key in properties:
                errors.extend(validate(value, properties[key], f"{path}.{key}", root))
            elif schema.get("additionalProperties") is False:
                errors.append(f"{path}: unexpected key {key!r}")
        if isinstance(schema.get("additionalProperties"), dict):
            for key, value in instance.items():
                if key not in properties:
                    errors.extend(
                        validate(value, schema["additionalProperties"], f"{path}.{key}", root)
                    )
    for keyword in ("oneOf", "anyOf"):
        if keyword in schema:
            matches = [not validate(instance, option, path, root) for option in schema[keyword]]
            if keyword == "oneOf" and sum(matches) != 1:
                errors.append(f"{path}: matched {sum(matches)} oneOf branches")
            if keyword == "anyOf" and not any(matches):
                errors.append(f"{path}: matched no anyOf branch")
    return errors


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        print("usage: validate_json_schema.py <schema.json> <instance.json>")
        return 2
    schema = json.load(open(argv[1], encoding="utf-8"))
    instance = json.load(open(argv[2], encoding="utf-8"))
    errors = validate(instance, schema)
    for error in errors:
        print(error)
    return int(bool(errors))


if __name__ == "__main__":
    import sys

    sys.exit(main(sys.argv))
