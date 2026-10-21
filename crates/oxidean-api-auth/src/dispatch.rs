//! Per-domain dispatch — see `oxidean-api-issues` for the contract. `None`
//! means the name is not this crate's; the shell tries the next dispatcher.

use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::{AppError, RpcResponse};
use serde_json::Value;

use crate::{bootstrap, local, two_factor, verify_reset};

macro_rules! arm {
    ($handler:expr) => {
        match $handler.await {
            Ok(v) => Some(RpcResponse::ok(v)),
            Err(e) => Some(RpcResponse::err(e)),
        }
    };
}

pub async fn dispatch(ctx: &mut RpcCtx, procedure: &str, input: Value) -> Option<RpcResponse> {
    match procedure {
        "auth.signup" => arm!(local::signup(ctx, input)),
        "auth.login" => arm!(local::login(ctx, input)),
        "auth.login_begin" => arm!(local::login_begin(ctx, input)),
        "auth.logout" => arm!(local::logout(ctx)),
        "auth.logout_all" => arm!(local::logout_all(ctx)),
        "auth.me" => arm!(local::me(ctx)),
        "auth.provider_config" => arm!(local::provider_config(ctx)),
        "auth.bootstrap_status" => arm!(bootstrap::bootstrap_status(ctx)),
        "auth.bootstrap_setup" => arm!(bootstrap::bootstrap_setup(ctx, input)),
        "auth.confirm_admin_credentials" => {
            arm!(bootstrap::confirm_admin_credentials(ctx, input))
        }
        "auth.verify" => arm!(verify_reset::verify(ctx, input)),
        "auth.request_verify" => arm!(verify_reset::request_verify(ctx)),
        "auth.resend_verify" => arm!(verify_reset::resend_verify(ctx)),
        "auth.request_password_reset" => {
            arm!(verify_reset::request_password_reset(ctx, input))
        }
        "auth.reset_password" => arm!(verify_reset::reset_password(ctx, input)),
        "auth.dev.privileged_ping" => {
            if !oxidean_api_core::auth::verify_reset::privileged_ping_env_allowed(&ctx.env_name) {
                Some(RpcResponse::err(AppError::new(
                    "rpc.unknown_procedure",
                    format!("unknown procedure: {procedure}"),
                )))
            } else {
                arm!(verify_reset::privileged_ping(ctx))
            }
        }
        "auth.two_factor.status" => arm!(two_factor::status(ctx)),
        "auth.two_factor.totp_begin" => arm!(two_factor::totp_begin(ctx, input)),
        "auth.two_factor.totp_confirm" => arm!(two_factor::totp_confirm(ctx, input)),
        "auth.two_factor.totp_disable" => arm!(two_factor::totp_disable(ctx)),
        "auth.two_factor.recovery_regenerate" => arm!(two_factor::recovery_regenerate(ctx)),
        "auth.two_factor.complete" => arm!(two_factor::complete(ctx, input)),
        "auth.two_factor.passkey_begin" => arm!(two_factor::passkey_begin(ctx, input)),
        "auth.two_factor.passkey_complete" => {
            arm!(two_factor::passkey_complete(ctx, input))
        }
        "auth.two_factor.passkey_register_begin" => {
            arm!(two_factor::passkey_register_begin(ctx, input))
        }
        "auth.two_factor.passkey_register_complete" => {
            arm!(two_factor::passkey_register_complete(ctx, input))
        }
        "auth.two_factor.passkey_update" => arm!(two_factor::passkey_update(ctx, input)),
        "auth.two_factor.passkey_delete" => arm!(two_factor::passkey_delete(ctx, input)),
        "auth.two_factor.email_send" => arm!(two_factor::email_send(ctx, input)),
        "admin.users.resetTwoFactor" => arm!(two_factor::admin_reset(ctx, input)),
        _ => None,
    }
}
