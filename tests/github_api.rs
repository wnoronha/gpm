use gpm::github::{GithubClient, ReleaseFetcher};
use gpm::network::ReqwestClient;
use serde_json::json;
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_github_pagination() {
    let mock_server = MockServer::start().await;

    // First page returns 1 release and a Link header for page 2
    Mock::given(method("GET"))
        .and(path("/repos/owner/repo/releases"))
        .and(query_param("per_page", "100"))
        .and(query_param_is_missing("page"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([
                    {
                        "tag_name": "v2.0",
                        "published_at": "2023-01-02T00:00:00Z",
                        "prerelease": false,
                        "draft": false,
                        "assets": []
                    }
                ]))
                .append_header(
                    "Link",
                    format!(
                        "<{}/repos/owner/repo/releases?per_page=100&page=2>; rel=\"next\"",
                        mock_server.uri()
                    )
                    .as_str(),
                ),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    // Second page returns 1 release and no Link header
    Mock::given(method("GET"))
        .and(path("/repos/owner/repo/releases"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "tag_name": "v1.0",
                "published_at": "2023-01-01T00:00:00Z",
                "prerelease": false,
                "draft": false,
                "assets": []
            }
        ])))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Set environment variable to point to mock server
    unsafe {
        std::env::set_var("GITHUB_API_URL", mock_server.uri());
    }

    let http = ReqwestClient::new().unwrap();
    let github = GithubClient::new(std::sync::Arc::new(http));

    let releases = github.get_releases("owner/repo").await.unwrap();

    assert_eq!(releases.len(), 2);
    assert_eq!(releases[0].tag_name, "v2.0");
    assert_eq!(releases[1].tag_name, "v1.0");
}
