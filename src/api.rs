use log::{info, trace, warn};
use regex::Regex;
use serde_json::{json, Value};

use crate::domain::auth::Auth;
use crate::domain::pullrequest::Request;
use crate::domain::users::User;
use crate::error::{Error, Result};

struct Server {
    host: String,
    project: String,
    repo: String,
}

pub fn submit_pull_request(request: &Request, push_url: &str, auth: &Auth) -> Result<String> {
    let server = get_stash_server_info(push_url)?;

    info!("Creating the pull request");
    let json_request = create_stash_rest_api_pr_request(&request, &server);
    let url = get_stash_rest_api_pr_url(&server);
    trace!("Sending request to {}\n {}", &url, json_request);

    let mut res = reqwest::Client::new()
        .post(&url)
        .basic_auth(&auth.user, auth.password.clone())
        .json(&json_request)
        .send()
        .map_err(|e| Error::new("Request failed", e))?;
    trace!("Got response: {:?}", res);
    let content = res
        .json::<Value>()
        .map_err(|e| Error::new("Can't parse response with json type", e))?;
    trace!("Got Content: {}", content);
    let url = &content["links"]["self"][0]["href"].as_str().unwrap();
    info!("Pull request {} is created!", url);

    Ok((*url).to_string())
}

pub fn find_users(filter: &str, push_url: &str, auth: &Auth) -> Result<Vec<User>> {
    let server = get_stash_server_info(push_url)?;

    info!("Query users: {}", filter);
    let url = get_stash_rest_api_users_url(&server, filter);
    trace!("Sending request to {}\n", &url);

    let mut res = reqwest::Client::new()
        .get(&url)
        .basic_auth(&auth.user, auth.password.clone())
        .send()
        .map_err(|e| Error::new("Request failed", e))?;
    trace!("Got response: {:?}", res);

    let mut users = vec![];
    match res.status() {
        reqwest::StatusCode::OK => {
            let content = res
                .json::<Value>()
                .map_err(|e| Error::new("Can't parse response with json type", e))?;
            trace!("Got Content: {}", content);

            for value in content["values"].as_array().unwrap() {
                users.push(User {
                    name: value["name"].as_str().unwrap().to_string(),
                    display_name: value["displayName"].as_str().unwrap().to_string(),
                    email: value["emailAddress"].as_str().unwrap().to_string(),
                })
            }
            info!("Below users are returned.\n{:?}", &users);
        }
        _ => {
            info!("No user found by filter {}", filter);
        }
    }

    Ok(users)
}

fn get_stash_server_info(push_url: &str) -> Result<Server> {
    match Regex::new("^ssh://git@(?P<host>[^/]+)/(?P<project>[^/]+)/(?P<repo>[^/]+)\\.git$")
        .unwrap()
        .captures_iter(push_url)
        .next()
    {
        Some(caps) => Ok(Server {
            host: caps["host"].to_string(),
            project: caps["project"].to_string(),
            repo: caps["repo"].to_string(),
        }),
        _ => {
            warn!("Can't get git origin remote info, please use git remote set-url origin ssh://... to setup it");
            Err(Error::from_str("No correct origin git remote url set"))
        }
    }
}
fn get_stash_rest_api_users_url(server: &Server, filter: &str) -> String {
    format!("{}/users?filter={}", &get_stash_rest_api(server), filter)
}
fn get_stash_rest_api_pr_url(server: &Server) -> String {
    format!(
        "{}/projects/{}/repos/{}/pull-requests",
        &get_stash_rest_api(server),
        &server.project,
        &server.repo
    )
}
fn get_stash_rest_api(server: &Server) -> String {
    format!("https://{}/rest/api/1.0", &server.host)
}

fn create_stash_rest_api_pr_request(request: &Request, server: &Server) -> Value {
    json!({
    "title": request.title,
    "description": request.description,
    "state": "OPEN",
    "open": true,
    "closed": false,
    "fromRef": {
        "id": format!("refs/heads/{}", request.branch_name),
        "repository": {
            "slug": &server.repo,
            "name": null,
            "project": {
                "key": &server.project
            }
        }
    },
    "toRef": {
        "id": format!("refs/heads/{}", request.to_branch),
        "repository": {
            "slug": &server.repo,
            "name": null,
            "project": {
                "key": &server.project
            }
        }
    },
    "locked": false,
    "reviewers": request.reviewers.iter().flat_map(|rs| rs.lines().flat_map(|l| l.split(';'))).map(|r| r.trim()).filter(|r| !r.is_empty()).map(|u| json!(
        {
            "user": {
                "name": u
            }
        })
    ).collect::<Vec<_>>()
    })
}
