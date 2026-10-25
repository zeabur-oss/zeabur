//! How admin accounts sign in: Zeabur Identity's install-wide settings, kept in its ConfigMap
//! `zeabur-identity-settings` and read by Zeabur Identity at each sign-in and refresh.
//!
//! - `teams`: `single` (the default) or `multiple`. With several teams, an idle admin session ends
//!   after 15 minutes instead of an hour.
//! - `admin-aal3`: `false` (the default) or `true`. When on, admin accounts sign in only with a
//!   device-bound passkey, and an idle admin session ends after 15 minutes. NIST SP 800-63B-4
//!   allows synced passkeys up to AAL2 only ([sp800-63b]). The authenticator reports whether a
//!   passkey can be synced, and nothing verifies it.
//!
//! A value Zeabur Identity does not understand gives the stricter setting, and so it does here.
//! The ConfigMap is read and written as the person, so RBAC decides who may change it. Turning
//! on `admin-aal3` is refused unless the person is an admin account with a device-bound
//! passkey, so that they can still sign in after it.
//!
//! [sp800-63b]: https://pages.nist.gov/800-63-4/sp800-63b.html

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use zeabur_core::problem::{Kind, MEDIA_TYPE as PROBLEM, Problem};
use zeabur_kube::Cluster;
use zeabur_model::Origin;

use super::store::{Stored, can_change_config_map, read_config_map, write_config_map_key};
use crate::error::ApiError;
use crate::handlers::{AppState, Person};
use crate::members::handlers::you;
use crate::members::has_device_bound_passkey;
use crate::projects::namespace::Permission;
use crate::views::UnreadableView;

/// The ConfigMap Zeabur Identity reads its settings from, in its namespace.
pub const CONFIG_MAP: &str = "zeabur-identity-settings";
const TEAMS_KEY: &str = "teams";
const ADMIN_AAL3_KEY: &str = "admin-aal3";

/// How many teams share the cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Teams {
    /// One team. An idle admin session ends after an hour.
    Single,
    /// Several teams. An idle admin session ends after 15 minutes.
    Multiple,
}

impl Teams {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Multiple => "multiple",
        }
    }
}

/// How admin accounts sign in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignInSetting {
    /// `settings/sign-in`.
    pub id: String,
    /// How many teams share the cluster.
    pub teams: Teams,
    /// Admin accounts sign in only with a device-bound passkey.
    pub admin_aal3: bool,
    /// The ConfigMap the settings are kept in.
    pub stored_in: Origin,
    /// Whether the person may change them, as far as their RBAC rules in that namespace tell.
    pub can_change: Permission,
    /// Whether the person is an admin account with a device-bound passkey, which turning on
    /// `adminAal3` needs.
    pub device_bound_passkey: bool,
    /// The ConfigMap, when it could not be read.
    pub unreadable: Vec<UnreadableView>,
}

/// A change of how admin accounts sign in. A field left out stays as it is.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignInChange {
    /// How many teams share the cluster.
    #[serde(default)]
    pub teams: Option<Teams>,
    /// Admin accounts sign in only with a device-bound passkey.
    #[serde(default)]
    pub admin_aal3: Option<bool>,
}

/// The settings in the ConfigMap. A value that is not understood gives the stricter setting.
fn parse(stored: &Stored) -> (Teams, bool) {
    let teams = match stored.get(TEAMS_KEY).map(str::trim) {
        None | Some("single") => Teams::Single,
        Some(_) => Teams::Multiple,
    };
    let admin_aal3 = !matches!(
        stored.get(ADMIN_AAL3_KEY).map(str::trim),
        None | Some("false")
    );
    (teams, admin_aal3)
}

async fn view(cluster: &dyn Cluster, namespace: &str) -> SignInSetting {
    let (stored, you) = futures_util::join!(
        read_config_map(cluster, namespace, CONFIG_MAP),
        you(cluster)
    );
    let (can_change, device_bound_passkey) = futures_util::join!(
        can_change_config_map(cluster, namespace, CONFIG_MAP, stored.object.is_some()),
        has_device_bound_passkey(cluster, namespace, &you)
    );
    let (teams, admin_aal3) = parse(&stored);
    SignInSetting {
        id: "settings/sign-in".to_owned(),
        teams,
        admin_aal3,
        stored_in: Origin::complete("v1", "ConfigMap", Some(namespace), CONFIG_MAP),
        can_change,
        device_bound_passkey: device_bound_passkey.unwrap_or(false),
        unreadable: stored.unreadable.into_iter().collect(),
    }
}

/// How admin accounts sign in, and whether you may change it.
#[utoipa::path(get, path = "/api/v1/settings/sign-in", tag = "settings",
    operation_id = "getSignInSetting",
    responses((status = 200, body = SignInSetting),
              (status = 401, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn get_setting(
    State(state): State<AppState>,
    Person(cluster): Person,
) -> Json<SignInSetting> {
    Json(view(cluster.as_ref(), &state.settings.identity.namespace).await)
}

/// Changes how admin accounts sign in, by writing Zeabur Identity's settings ConfigMap as you
/// (created the first time). Who may: whoever may update (and create) the ConfigMap
/// `zeabur-identity-settings` in Zeabur Identity's namespace. Turning on `adminAal3` is refused
/// unless you are an admin account with a device-bound passkey.
#[utoipa::path(put, path = "/api/v1/settings/sign-in", tag = "settings",
    operation_id = "setSignInSetting",
    request_body = SignInChange,
    responses((status = 200, body = SignInSetting),
              (status = 403, body = Problem, content_type = PROBLEM),
              (status = 409, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn set(
    State(state): State<AppState>,
    Person(cluster): Person,
    Json(change): Json<SignInChange>,
) -> Result<Json<SignInSetting>, ApiError> {
    let cluster = cluster.as_ref();
    let namespace = state.settings.identity.namespace.as_str();
    if change.admin_aal3 == Some(true) {
        let you = you(cluster).await;
        if !has_device_bound_passkey(cluster, namespace, &you).await? {
            let mut problem = Problem::new(
                Kind::Conflict,
                "You have no device-bound passkey, so you could not sign in after this change.",
            );
            problem.summary = zeabur_core::message::Message::new("settings.signIn.noDeviceBound");
            problem.fix_for_occasion(
                "Add a passkey kept on a security key or this device, then try again.",
                zeabur_core::message::Message::new("settings.signIn.noDeviceBound.fix"),
            );
            return Err(ApiError::from(problem));
        }
    }
    let writes = [
        (TEAMS_KEY, change.teams.map(Teams::as_str)),
        (
            ADMIN_AAL3_KEY,
            change
                .admin_aal3
                .map(|on| if on { "true" } else { "false" }),
        ),
    ];
    for (key, value) in writes {
        let Some(value) = value else { continue };
        let stored = read_config_map(cluster, namespace, CONFIG_MAP).await;
        write_config_map_key(
            cluster,
            namespace,
            CONFIG_MAP,
            stored.object.as_ref(),
            key,
            Some(value),
        )
        .await?;
    }
    Ok(Json(view(cluster, namespace).await))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn stored(data: &serde_json::Value) -> Stored {
        Stored {
            object: Some(json!({"data": data})),
            unreadable: None,
        }
    }

    #[test]
    fn a_missing_setting_is_the_default_and_an_unknown_one_the_stricter() {
        assert_eq!(parse(&Stored::default()), (Teams::Single, false));
        assert_eq!(
            parse(&stored(&json!({"teams": "multiple", "admin-aal3": "true"}))),
            (Teams::Multiple, true)
        );
        assert_eq!(
            parse(&stored(&json!({"teams": "many", "admin-aal3": "yes"}))),
            (Teams::Multiple, true)
        );
        assert_eq!(
            parse(&stored(&json!({"teams": "single", "admin-aal3": "false"}))),
            (Teams::Single, false)
        );
    }
}
