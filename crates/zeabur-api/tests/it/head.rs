//! Whether a person may open a page: HEAD answers it from the page's own reads (RFC 9110
//! section 9.3.2), and GET refuses when none of them is allowed.

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use zeabur_kube::memory::{MemoryCluster, State};

    use crate::common::{app, send};

    const BACKUPS: &str = "/api/v1/cluster/backups";
    const NODES: &str = "/api/v1/cluster/nodes";
    const MEMBERS: &str = "/api/v1/members";

    fn refusing(rights: &[&str]) -> MemoryCluster {
        let cluster = MemoryCluster::new(State::default());
        cluster.update(|s| {
            s.refused_rights
                .extend(rights.iter().map(|right| (*right).to_owned()));
        });
        cluster
    }

    #[tokio::test]
    async fn head_opens_when_any_read_is_allowed() {
        let cluster = refusing(&["list schedules", "list backups", "list configmaps"]);
        let (status, body) = send(&app(&cluster), "HEAD", BACKUPS, None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.is_null());
    }

    #[tokio::test]
    async fn head_and_get_refuse_when_no_read_is_allowed() {
        let cluster = refusing(&[
            "list schedules",
            "list backups",
            "list backupstoragelocations",
            "list configmaps",
        ]);
        let app = app(&cluster);
        let (status, body) = send(&app, "HEAD", BACKUPS, None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(body.is_null());
        let (status, _) = send(&app, "GET", BACKUPS, None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn head_of_the_overview_follows_the_nodes_read() {
        let app = app(&refusing(&["list nodes"]));
        let (status, _) = send(&app, "HEAD", NODES, None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _) = send(&app, "HEAD", MEMBERS, None).await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn members_opens_for_api_tokens_without_identity_rights() {
        let identity = [
            "list users",
            "list credentials",
            "list invitations",
            "list grants",
        ];
        let server = app(&refusing(&identity));
        let (status, _) = send(&server, "HEAD", MEMBERS, None).await;
        assert_eq!(status, StatusCode::OK);
        let (status, body) = send(&server, "GET", MEMBERS, None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["installed"], false);

        let tokens = [
            &identity[..],
            &["list rolebindings", "list serviceaccounts"],
        ]
        .concat();
        let (status, _) = send(&app(&refusing(&tokens)), "HEAD", MEMBERS, None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
}
