//! `oxpilot.status` — the gate-exempt enablement probe (BR2.3). The
//! flag-off UI calls it to learn `reason`; it never exposes env values,
//! filesystem paths, or secret material — just enabled/provider/reason.

use oxidean_api_core::oxpilot::resolve_enablement;
use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::AppError;
use serde_json::{json, Value};

fn require_session(ctx: &RpcCtx) -> Result<(), AppError> {
    if ctx.session.is_none() {
        return Err(AppError::new("auth.unauthenticated", "not authenticated"));
    }
    Ok(())
}

pub async fn status(ctx: &RpcCtx) -> Result<Value, AppError> {
    require_session(ctx)?;
    let st = resolve_enablement(ctx).await;
    Ok(json!({
        "enabled": st.enabled,
        "provider": st.provider.map(|(id, model)| json!({ "id": id, "model": model })),
        "reason": st.reason,
    }))
}
