//! Profile CRUD RPC (`user.get_profile` / `user.update_profile`) — AUTH-08, D-18.

use oxidean_core::{validate_username, AppError, UpdateProfileRequest, UserPublic};

use crate::auth::local::user_to_public;
use crate::rpc::RpcCtx;

const BIO_MAX_CHARS: usize = 160;

fn map_username_err(msg: String) -> AppError {
    if msg.contains("reserved") {
        AppError::new("auth.reserved_username", "username is reserved")
    } else {
        AppError::new("auth.invalid_username", msg)
    }
}

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else if e.contains("UNIQUE") || e.contains("unique") || e.contains("Duplicate") {
        AppError::new("auth.taken", "email or username already taken")
    } else {
        tracing::error!("profile db error: {e}");
        AppError::new("auth.internal", "profile operation failed")
    }
}

fn require_session(ctx: &RpcCtx) -> Result<&crate::auth::session::ResolvedSession, AppError> {
    ctx.session.as_ref().ok_or_else(|| {
        AppError::new("auth.unauthenticated", "not authenticated")
    })
}

/// `user.get_profile` — current user's public profile.
pub async fn get_profile(ctx: &RpcCtx) -> Result<UserPublic, AppError> {
    let session = require_session(ctx)?;
    let user = ctx
        .db
        .find_user_by_id(&session.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))?;
    if user.banned_at.is_some() {
        return Err(AppError::new(
            "auth.banned",
            "This account has been suspended.",
        ));
    }
    Ok(user_to_public(&user))
}

/// `user.getPublicProfile` — public profile by username; never includes email (D-SOC-06/08).
pub async fn get_public_profile(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<oxidean_core::PublicUserProfile, AppError> {
    let req: oxidean_core::GetPublicProfileRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid user.getPublicProfile input: {e}"),
        )
    })?;
    let username = req.username.trim();
    if username.is_empty() {
        return Err(AppError::new("user.not_found", "User not found"));
    }
    let user = ctx
        .db
        .find_user_by_username(username)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("user.not_found", "User not found"))?;
    if user.banned_at.is_some() {
        return Err(AppError::new("user.not_found", "User not found"));
    }
    Ok(oxidean_core::PublicUserProfile {
        username: user.username,
        display_name: user.display_name,
        bio: user.bio,
        avatar_url: user.avatar_path,
    })
}

/// `user.update_profile` — display name, username, bio (avatar via multipart route).
pub async fn update_profile(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<UserPublic, AppError> {
    let session = require_session(ctx)?;
    let req: UpdateProfileRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid update_profile input: {e}"))
    })?;

    let username = req.username.trim().to_string();
    validate_username(&username).map_err(map_username_err)?;

    let display_name = req.display_name.trim().to_string();
    if display_name.is_empty() || display_name.len() > 100 {
        return Err(AppError::new(
            "auth.invalid_display_name",
            "display name must be 1–100 characters",
        ));
    }

    let bio = req.bio;
    if bio.chars().count() > BIO_MAX_CHARS {
        return Err(AppError::new(
            "auth.invalid_bio",
            format!("bio must be at most {BIO_MAX_CHARS} characters"),
        ));
    }

    let existing = ctx
        .db
        .find_user_by_id(&session.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))?;

    if username != existing.username {
        return Err(AppError::new(
            "auth.username_immutable",
            "Usernames cannot be changed",
        ));
    }

    let updated = match ctx
        .db
        .update_user_profile(
            &session.user_id,
            &display_name,
            &existing.username,
            &bio,
            existing.avatar_path.as_deref(),
        )
        .await
    {
        Ok(u) => u,
        Err(e) => return Err(db_err(e)),
    };

    let updated = if let Some(branch) = req.default_branch {
        let branch = branch.trim().to_string();
        if branch.is_empty() || branch.len() > 100 {
            return Err(AppError::new(
                "auth.invalid_default_branch",
                "default branch name must be 1–100 characters",
            ));
        }
        if !branch
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '/' || c == '.')
        {
            return Err(AppError::new(
                "auth.invalid_default_branch",
                "default branch name has invalid characters",
            ));
        }
        ctx.db
            .set_user_default_branch(&session.user_id, &branch)
            .await
            .map_err(db_err)?
    } else {
        updated
    };

    Ok(user_to_public(&updated))
}
