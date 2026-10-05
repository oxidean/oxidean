//! Single-source route table for `/api/v1` — drives both the Axum router and
//! the generated OpenAPI document, so the spec cannot drift from the served
//! surface. `GET /api/v1/openapi.json` renders [`openapi_doc`]; the Swagger UI
//! at `/api/v1/docs` points at it.

use std::collections::{BTreeMap, BTreeSet};

use axum::http::StatusCode;
use axum::routing::MethodRouter;
use axum::Router;
use schemars::{Schema, SchemaGenerator};
use serde_json::{json, Map, Value};

use crate::app::AppState;

use super::{admin, issues, orgs, pulls, releases, repos, statuses, users};

/// `T` → component `$ref`; `SchemaGenerator::subschema_for::<T>` as a plain
/// fn pointer so tables stay `const`-constructible. Defs accumulate in the
/// shared generator and land in `components.schemas`.
pub type SchemaFn = fn(&mut SchemaGenerator) -> Schema;

/// `T` → root schema (inline object + embedded `$defs`);
/// `SchemaGenerator::into_root_schema_for::<T>` as a fn pointer. Used for
/// bodies/queries where route-level field stripping needs the root object.
pub type RootFn = fn(SchemaGenerator) -> Schema;

/// Request-body schema for a route.
#[derive(Clone, Copy)]
pub enum BodySpec {
    /// JSON Schema generated from the RPC request type, with the route's
    /// `path_fields` removed — they come from the URL.
    Rpc(RootFn),
    /// Hand-written schema for bodies that don't map 1:1 to one RPC request
    /// (e.g. PATCH fan-out to close/reopen, GitHub-shaped payloads).
    Json(fn(&mut SchemaGenerator) -> Value),
}

/// Success-response schema for a route.
#[derive(Clone, Copy)]
pub enum RespSpec {
    /// `$ref` to the component generated from the RPC response type.
    Schema(SchemaFn),
    /// Hand-written schema (GitHub-compatible shapes, mapped payloads).
    Json(fn(&mut SchemaGenerator) -> Value),
}

/// One REST operation: route registration + OpenAPI metadata together.
pub struct RouteDef {
    pub method: &'static str,
    /// Axum/OpenAPI path template (`{param}`, `{*wildcard}` → `{param}`).
    pub path: &'static str,
    pub tags: &'static [&'static str],
    pub operation_id: &'static str,
    /// Doc-line; convention: ends with the backing `proc.name` in parens.
    pub summary: &'static str,
    /// Primary RPC procedure — asserted against [`rpc::PROCEDURES`] in tests.
    pub procedure: &'static str,
    pub ok: StatusCode,
    /// Callable with no credentials at all (e.g. health, public reads).
    pub anonymous: bool,
    /// Serialized request-field names populated from the URL path
    /// (`owner`, `name`, `number`, …) — stripped from body/query schemas.
    pub path_fields: &'static [&'static str],
    /// Query-params object schema (a request type or local `*Query` struct).
    pub query: Option<RootFn>,
    pub body: Option<BodySpec>,
    pub response: Option<RespSpec>,
    /// `|| get(handler)` — the axum registration for this operation.
    pub mount: fn() -> MethodRouter<AppState>,
}

/// Every `/api/v1` operation, in declaration order.
pub fn route_defs() -> &'static [&'static [RouteDef]] {
    &[
        super::META_ROUTES,
        users::ROUTES,
        orgs::ROUTES,
        repos::ROUTES,
        statuses::ROUTES,
        issues::ROUTES,
        pulls::ROUTES,
        releases::ROUTES,
        admin::ROUTES,
    ]
}

/// Build the `/api/v1` router from the same table the spec renders.
pub fn router_from_defs() -> Router<AppState> {
    // Group by path so one axum `.route` call gets the merged MethodRouter.
    let mut by_path: BTreeMap<&'static str, MethodRouter<AppState>> = BTreeMap::new();
    for def in route_defs().iter().flat_map(|d| d.iter()) {
        let mr = (def.mount)();
        by_path
            .entry(def.path)
            .and_modify(|m| *m = m.clone().merge(mr.clone()))
            .or_insert(mr);
    }
    let mut router = Router::new();
    for (path, mr) in by_path {
        router = router.route(path, mr);
    }
    router
}

/// `{name}` / `{*name}` captures, in order of appearance.
fn path_params(path: &str) -> Vec<&str> {
    path.split('{')
        .skip(1)
        .map(|seg| seg.split('}').next().unwrap_or("").trim_start_matches('*'))
        .collect()
}

/// Convert a query-param object schema into OpenAPI `parameters`, skipping
/// fields the URL path already supplies.
fn query_params(schema: &Value, skip: &BTreeSet<&str>) -> Vec<Value> {
    let Some(props) = schema.get("properties").and_then(|v| v.as_object()) else {
        return Vec::new();
    };
    let required: BTreeSet<&str> = schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    props
        .iter()
        .filter(|(name, _)| !skip.contains(name.as_str()))
        .map(|(name, sub)| {
            json!({
                "name": name,
                "in": "query",
                "required": required.contains(name.as_str()),
                "schema": sub,
            })
        })
        .collect()
}

/// Strip URL-populated fields out of a generated body schema's root object.
fn strip_path_fields(schema: &mut Value, fields: &[&str]) {
    let Some(obj) = schema.as_object_mut() else {
        return;
    };
    if let Some(props) = obj.get_mut("properties").and_then(|v| v.as_object_mut()) {
        for f in fields {
            props.remove(*f);
        }
    }
    if let Some(req) = obj.get_mut("required").and_then(|v| v.as_array_mut()) {
        let owned: BTreeSet<&str> = fields.iter().copied().collect();
        req.retain(|v| v.as_str().is_none_or(|s| !owned.contains(s)));
    }
}

/// Move a generated schema's inline `$defs` into `components` and return the
/// root schema (or the `$ref` it already is).
fn hoist_defs(schema: &mut Value, components: &mut Map<String, Value>) {
    if let Some(defs) = schema.as_object_mut().and_then(|o| o.remove("$defs")) {
        if let Value::Object(defs) = defs {
            for (k, v) in defs {
                components.insert(k, v);
            }
        }
    }
}

/// A body schema for `T` minus the fields that arrive via the URL.
fn body_schema(
    gen: &mut SchemaGenerator,
    spec: BodySpec,
    path_fields: &'static [&'static str],
    components: &mut Map<String, Value>,
) -> Value {
    match spec {
        BodySpec::Rpc(f) => {
            // Fresh generator → a self-contained root schema with `$defs`.
            let mut v = serde_json::to_value(f(SchemaGenerator::default())).unwrap_or(Value::Null);
            hoist_defs(&mut v, components);
            // Named types may render root as `{"$ref":"#/$defs/T"}` — strip
            // path fields inside the def rather than on the ref shell.
            if let Some(name) = v
                .get("$ref")
                .and_then(|r| r.as_str())
                .and_then(|r| r.strip_prefix("#/$defs/"))
                .map(str::to_string)
            {
                if let Some(def) = components.get_mut(&name) {
                    strip_path_fields(def, path_fields);
                }
            } else {
                strip_path_fields(&mut v, path_fields);
            }
            v
        }
        BodySpec::Json(f) => f(gen),
    }
}

fn response_schema(gen: &mut SchemaGenerator, spec: RespSpec) -> Value {
    match spec {
        RespSpec::Schema(f) => serde_json::to_value(f(gen)).unwrap_or(Value::Null),
        RespSpec::Json(f) => f(gen),
    }
}

/// Rewrite schemars `#/$defs/…` refs to OpenAPI `#/components/schemas/…` and
/// drop draft-2020-12 `$schema` markers (out of spec inside OpenAPI 3.0).
fn rewrite_refs(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.remove("$schema");
            for (k, val) in m.iter_mut() {
                if k == "$ref" {
                    if let Some(s) = val.as_str() {
                        if let Some(rest) = s.strip_prefix("#/$defs/") {
                            *val = json!(format!("#/components/schemas/{rest}"));
                        }
                    }
                } else {
                    rewrite_refs(val);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(rewrite_refs),
        _ => {}
    }
}

fn error_schema() -> Value {
    json!({
        "type": "object",
        "required": ["code", "message"],
        "properties": {
            "code": { "type": "string", "description": "domain.error_code" },
            "message": { "type": "string" },
            "data": { "description": "optional structured detail" },
        },
    })
}

/// The complete OpenAPI 3.0 document for `/api/v1`.
pub fn openapi_doc() -> Value {
    let mut gen = SchemaGenerator::default();
    let mut components: Map<String, Value> = Map::new();
    components.insert("Error".into(), error_schema());

    let mut paths: Map<String, Value> = Map::new();
    for def in route_defs().iter().flat_map(|d| d.iter()) {
        let mut op = Map::new();
        op.insert("tags".into(), json!(def.tags));
        op.insert("summary".into(), json!(def.summary));
        op.insert("operationId".into(), json!(def.operation_id));
        if def.anonymous {
            op.insert("security".into(), json!([]));
        }

        let skip: BTreeSet<&str> = def.path_fields.iter().copied().collect();
        let mut params = Vec::new();
        for name in path_params(def.path) {
            let schema = if name == "number" {
                json!({ "type": "integer" })
            } else {
                json!({ "type": "string" })
            };
            params.push(json!({
                "name": name,
                "in": "path",
                "required": true,
                "schema": schema,
            }));
        }
        if let Some(q) = def.query {
            let mut qs = serde_json::to_value(q(SchemaGenerator::default())).unwrap_or(Value::Null);
            hoist_defs(&mut qs, &mut components);
            // Core request types carry path-populated fields (`owner`, `name`)
            // that the local `*Query` structs don't — filter either way.
            params.extend(query_params(&qs, &skip));
        }
        if !params.is_empty() {
            op.insert("parameters".into(), Value::Array(params));
        }

        if let Some(bs) = def.body {
            let mut schema = body_schema(&mut gen, bs, def.path_fields, &mut components);
            hoist_defs(&mut schema, &mut components);
            op.insert(
                "requestBody".into(),
                json!({
                    "required": true,
                    "content": { "application/json": { "schema": schema } },
                }),
            );
        }

        let mut responses = Map::new();
        let ok_desc = match def.ok {
            StatusCode::CREATED => "Created",
            _ => "OK",
        };
        let mut ok_obj = Map::new();
        ok_obj.insert("description".into(), json!(ok_desc));
        if let Some(resp) = def.response {
            let mut rs = response_schema(&mut gen, resp);
            hoist_defs(&mut rs, &mut components);
            ok_obj.insert(
                "content".into(),
                json!({ "application/json": { "schema": rs } }),
            );
        }
        responses.insert(def.ok.as_str().to_string(), Value::Object(ok_obj));
        responses.insert(
            "default".into(),
            json!({
                "description": "Error — `{ code, message, data? }` with a mapped HTTP status",
                "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Error" } } },
            }),
        );
        op.insert("responses".into(), Value::Object(responses));

        let path = def.path.replace("{*", "{");
        let entry = paths.entry(path).or_insert_with(|| json!({}));
        entry
            .as_object_mut()
            .unwrap()
            .insert(def.method.to_ascii_lowercase(), Value::Object(op));
    }

    // Remaining generator-side defs (subschema_for refs) merge last.
    for (k, v) in gen.definitions().clone() {
        components.entry(k).or_insert(v);
    }

    let mut doc = json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Oxidean REST API",
            "version": "1",
            "description": "Public REST API for Oxidean — a companion to the typed JSON RPC at `POST /api/rpc`. Each route dispatches to the same internal procedure the RPC layer uses, so ACLs, PAT scope gates, and side effects are identical. Generated from the route table — this document cannot drift from the served surface.",
        },
        "servers": [{ "url": "/api/v1" }],
        "security": [{ "sessionCookie": [] }, { "bearerPat": [] }],
        "tags": [
            { "name": "meta" },
            { "name": "users" },
            { "name": "orgs" },
            { "name": "repos" },
            { "name": "issues" },
            { "name": "pulls" },
            { "name": "releases" },
            { "name": "admin" },
        ],
        "paths": paths,
        "components": {
            "schemas": components,
            "securitySchemes": {
                "sessionCookie": {
                    "type": "apiKey",
                    "in": "cookie",
                    "name": "oxidean_session",
                    "description": "Browser session cookie — wins if both credentials are sent.",
                },
                "bearerPat": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "Personal access token. Classic `repo` covers the repo domain; `admin.*` and org/credential mutations are session-only.",
                },
            },
        },
    });
    rewrite_refs(&mut doc);
    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc;

    /// Every documented operation must name a real RPC procedure — catches
    /// typos that would otherwise render a spec for a route that cannot work.
    #[test]
    fn every_route_procedure_is_registered() {
        let known: BTreeSet<&str> = rpc::PROCEDURES.iter().copied().collect();
        for def in route_defs().iter().flat_map(|d| d.iter()) {
            assert!(
                known.contains(def.procedure),
                "{} {} → `{}` is not in rpc::PROCEDURES",
                def.method,
                def.path,
                def.procedure
            );
        }
    }

    /// The route table is the router — duplicates would panic in axum.
    #[test]
    fn no_duplicate_method_path_pairs() {
        let mut seen = BTreeSet::new();
        for def in route_defs().iter().flat_map(|d| d.iter()) {
            assert!(
                seen.insert((def.method, def.path)),
                "duplicate route {} {}",
                def.method,
                def.path
            );
        }
    }

    /// The generated document must be internally consistent: unique
    /// operationIds, every op has responses, and every
    /// `#/components/schemas/*` ref resolves.
    #[test]
    fn openapi_doc_is_self_consistent() {
        let doc = openapi_doc();

        let schemas: BTreeSet<&str> = doc["components"]["schemas"]
            .as_object()
            .expect("components.schemas")
            .keys()
            .map(String::as_str)
            .collect();
        assert!(schemas.contains("Error"));

        let mut operation_ids = BTreeSet::new();
        let paths = doc["paths"].as_object().expect("paths object");
        assert!(!paths.is_empty());
        for (path, item) in paths {
            for (method, op) in item.as_object().expect("path item") {
                assert!(
                    op["responses"].as_object().is_some_and(|r| !r.is_empty()),
                    "{method} {path} has no responses"
                );
                let id = op["operationId"].as_str().expect("operationId");
                assert!(operation_ids.insert(id), "duplicate operationId {id}");
            }
        }
        assert!(operation_ids.len() > 40, "expected ~60 ops");

        // Every components ref must resolve.
        let text = serde_json::to_string(&doc).unwrap();
        for m in text.match_indices("#/components/schemas/") {
            let rest = &text[m.0 + 21..];
            let name = rest.split('"').next().unwrap_or("");
            assert!(
                schemas.contains(name),
                "dangling schema ref {name} in openapi doc"
            );
        }
    }

    /// Building the router must not panic (axum rejects overlapping
    /// method+path registrations at construction time).
    #[test]
    fn router_builds() {
        let _ = router_from_defs();
    }
}
