use serde::Deserialize;
use serde_json::Value;
use std::env;
use std::error::Error;
use std::io::{self, BufRead, Write};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::Serialize;
use serde_json::json;

#[derive(Deserialize, Debug)]
struct PullRequest {
    number: u32,
    title: String,
    body: Option<String>,
    user: User,
    created_at: String,
    html_url: String,
    comments_url: String,
    url: String,
}

#[derive(Deserialize, Debug)]
struct User {
    login: String,
}

#[derive(Deserialize, Debug)]
struct Comment {
    id: u64,
    user: User,
    created_at: String,
    body: String,
}

#[derive(Serialize, Debug)]
struct ClaudeMessage {
    role: String,
    content: String,
}

#[derive(Serialize, Debug)]
struct ClaudeRequest {
    model: String,
    messages: Vec<ClaudeMessage>,
    max_tokens: u32,
}

fn get_comments_count(comments_url: &str) -> Result<usize, Box<dyn Error>> {
    let response = ureq::get(comments_url)
        .set("User-Agent", "rubber")
        .call()?
        .into_string()?;

    let comments: Vec<Value> = serde_json::from_str(&response)?;
    Ok(comments.len())
}

fn get_pr_comments(comments_url: &str) -> Result<Vec<Comment>, Box<dyn Error>> {
    let response = ureq::get(comments_url)
        .set("User-Agent", "rubber")
        .call()?
        .into_string()?;

    let comments: Vec<Comment> = serde_json::from_str(&response)?;
    Ok(comments)
}

#[derive(Deserialize, Debug)]
struct PullRequestDetail {
    title: String,
    body: Option<String>,
    html_url: String,
    #[serde(default)]
    files: Vec<FileChange>,
}

#[derive(Deserialize, Debug)]
struct FileChange {
    filename: String,
    status: String,
    additions: u32,
    deletions: u32,
    changes: u32,
    patch: Option<String>,
}

async fn get_code_review(patch: &str) -> Result<String, Box<dyn Error>> {
    let api_key = env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY environment variable not set");

    let prompt = format!(
        "Please review this code patch and provide specific, actionable feedback about potential issues, \
        improvements, and best practices. Consider performance, security, maintainability, and Rust idioms.\n\n\
        ```\n{}\n```",
        patch
    );

    let client = reqwest::Client::new();
    let mut headers = HeaderMap::new();
    headers.insert("x-api-key", HeaderValue::from_str(&api_key)?);
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        "anthropic-version",
        HeaderValue::from_static("2023-06-01"),
    );

    let messages = vec![ClaudeMessage {
        role: "user".to_string(),
        content: prompt,
    }];

    let request = ClaudeRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages,
        max_tokens: 1000,
    };

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .headers(headers)
        .json(&request)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    println!("Request: {:?}", request);
    println!("Response: {:?}", response);

    let review = response["content"][0]["text"]
        .as_str()
        .ok_or("Failed to get response text")?
        .to_string();

    Ok(review)
}

async fn analyze_patch(patch: &str) -> (String, Vec<String>, Vec<String>, Option<String>) {
    let mut summary = String::new();
    let mut questions = Vec::new();
    let mut comments = Vec::new();
    let mut claude_review = None;

    // Count additions and deletions
    let additions = patch.lines().filter(|l| l.starts_with('+')).count();
    let deletions = patch.lines().filter(|l| l.starts_with('-')).count();

    summary.push_str(&format!(
        "Changed {} lines ({} additions, {} deletions)\n",
        additions + deletions,
        additions,
        deletions
    ));

    // Look for common patterns that might need attention
    if patch.contains("TODO") || patch.contains("FIXME") {
        questions.push(
            "There are TODOs/FIXMEs in the code - should these be addressed before merging?".into(),
        );
    }

    if patch.contains("println!") || patch.contains("dbg!") {
        questions.push("Debug print statements found - are these intended for production?".into());
    }

    // Look for potential improvements
    if patch.contains("unwrap()") {
        comments.push("Consider handling errors explicitly instead of using unwrap()".into());
    }

    if patch.contains("panic!") {
        comments.push(
            "Consider if panic! is appropriate here or if errors should be handled gracefully"
                .into(),
        );
    }

    // Add Claude's review if available
    match get_code_review(patch).await {
        Ok(review) => claude_review = Some(review),
        Err(e) => println!("Error getting code review: {}", e),
    }

    (summary, questions, comments, claude_review)
}

fn get_pr_details(
    pr_number: u32,
    owner: &str,
    repo: &str,
) -> Result<PullRequestDetail, Box<dyn Error>> {
    // The correct endpoint for PR details
    let pr_url = format!(
        "https://api.github.com/repos/{}/{}/pulls/{}",
        owner, repo, pr_number
    );

    // Fetch PR details
    let pr_response = ureq::get(&pr_url)
        .set("User-Agent", "rubber")
        .call()?
        .into_string()?;

    let mut details: PullRequestDetail = serde_json::from_str(&pr_response)?;

    // Fetch files separately using the files endpoint
    let files_url = format!(
        "https://api.github.com/repos/{}/{}/pulls/{}/files",
        owner, repo, pr_number
    );

    let files_response = ureq::get(&files_url)
        .set("User-Agent", "rubber")
        .call()?
        .into_string()?;

    details.files = serde_json::from_str(&files_response)?;

    Ok(details)
}

async fn display_pr_details(pr: &PullRequest, owner: &str, repo: &str) -> Result<(), Box<dyn Error>> {
    println!("\n{}", "=".repeat(80));
    println!("PR #{}: {}", pr.number, pr.title);
    println!("{}", "=".repeat(80));

    println!("\nDescription:");
    println!("{}", "-".repeat(80));
    if let Some(body) = &pr.body {
        if !body.trim().is_empty() {
            println!("{}", body);
        } else {
            println!("No description provided.");
        }
    } else {
        println!("No description provided.");
    }
    println!("{}", "-".repeat(80));

    // Get the PR details including modified files
    match get_pr_details(pr.number, owner, repo) {
        Ok(details) => {
            println!("\nModified Files:");
            println!("{}", "-".repeat(80));

            if details.files.is_empty() {
                println!("No files modified in this PR.");
            } else {
                println!(
                    "{:<50} {:<10} {:<10} {:<10}",
                    "Filename", "Status", "Additions", "Deletions"
                );
                for file in details.files {
                    println!(
                        "{:<50} {:<10} {:<10} {:<10}",
                        file.filename, file.status, file.additions, file.deletions
                    );

                    // Display the diff/patch if available
                    if let Some(patch) = file.patch {
                        println!("\nDiff for {}:", file.filename);
                        println!("{}", "-".repeat(80));
                        println!("{}", patch);
                        println!("{}", "-".repeat(80));

                        // Analyze the patch
                        let (summary, questions, comments, claude_review) = analyze_patch(&patch).await;

                        println!("\nAnalysis:");
                        println!("Summary: {}", summary);

                        if !questions.is_empty() {
                            println!("\nQuestions to consider:");
                            for q in questions {
                                println!("- {}", q);
                            }
                        }

                        if !comments.is_empty() {
                            println!("\nPotential feedback:");
                            for c in comments {
                                println!("- {}", c);
                            }
                        }

                        if let Some(review) = claude_review {
                            println!("\nClaude's Review:");
                            println!("{}", "-".repeat(80));
                            println!("{}", review);
                            println!("{}", "-".repeat(80));
                        }

                        println!();
                    }
                }
            }
            println!("{}", "-".repeat(80));
        }
        Err(e) => {
            println!("\nError fetching PR details: {}", e);
            println!("Unable to display modified files.");
        }
    }

    Ok(())
}

fn display_pr_comments(pr_number: u32, owner: &str, repo: &str) -> Result<(), Box<dyn Error>> {
    let comments_url = format!(
        "https://api.github.com/repos/{}/{}/issues/{}/comments",
        owner, repo, pr_number
    );

    let comments = get_pr_comments(&comments_url)?;

    println!("\nComments for PR #{}:", pr_number);

    if comments.is_empty() {
        println!("No comments found for this PR.");
    } else {
        println!("{}", "-".repeat(80));
        for comment in comments {
            println!("Author: {} (at {})", comment.user.login, comment.created_at);
            println!("{}", "-".repeat(80));
            println!("{}", comment.body);
            println!("{}", "-".repeat(80));
            println!();
        }
    }

    Ok(())
}

fn find_pr_by_number(prs: &[PullRequest], number: u32) -> Option<&PullRequest> {
    prs.iter().find(|pr| pr.number == number)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <owner> <repo> [pr_number]", args[0]);
        std::process::exit(1);
    }

    let owner = &args[1];
    let repo = &args[2];
    
    // If PR number is provided, show its details directly
    if let Some(pr_number) = args.get(3) {
        match pr_number.parse::<u32>() {
            Ok(number) => {
                // Fetch the specific PR
                let pr_url = format!(
                    "https://api.github.com/repos/{}/{}/pulls/{}",
                    owner, repo, number
                );
                
                let pr = ureq::get(&pr_url)
                    .set("User-Agent", "rubber")
                    .call()?
                    .into_json::<PullRequest>()?;
                
                // Display PR details and comments
                display_pr_details(&pr, owner, repo).await?;
                display_pr_comments(number, owner, repo)?;
                return Ok(());
            }
            Err(_) => {
                eprintln!("Invalid PR number: {}", pr_number);
                std::process::exit(1);
            }
        }
    }

    // Original interactive flow for listing PRs
    println!("Fetching the 10 most recent PRs for {}/{}", owner, repo);

    let url = format!(
        "https://api.github.com/repos/{}/{}/pulls?state=all&sort=created&direction=desc&per_page=10",
        owner, repo
    );

    let response = ureq::get(&url)
        .set("User-Agent", "rubber")
        .call()?
        .into_json::<Vec<PullRequest>>()?;

    if response.is_empty() {
        println!("No pull requests found.");
    } else {
        println!(
            "{:<6} {:<50} {:<20} {:<15} {:<15}",
            "PR#", "Title", "Author", "Created At", "Comments"
        );
        println!("{}", "-".repeat(106));

        for pr in &response {
            // Truncate title if too long
            let title = if pr.title.len() > 47 {
                format!("{}...", &pr.title[..44])
            } else {
                pr.title.clone()
            };

            // Fetch comment count for this PR
            let comments_count = match get_comments_count(&pr.comments_url) {
                Ok(count) => count.to_string(),
                Err(_) => "Error".to_string(),
            };

            println!(
                "{:<6} {:<50} {:<20} {:<15} {:<15}",
                pr.number, title, pr.user.login, pr.created_at, comments_count
            );

            // Print the PR URL on a separate line
            println!("       URL: {}", pr.html_url);
        }

        println!("\nEnter PR number to view details (or 'q' to quit): ");
        io::stdout().flush()?;

        let stdin = io::stdin();
        let mut input = String::new();
        stdin.lock().read_line(&mut input)?;

        let input = input.trim();
        if input.to_lowercase() != "q" {
            match input.parse::<u32>() {
                Ok(pr_number) => {
                    if let Some(pr) = find_pr_by_number(&response, pr_number) {
                        // Display PR details (title, description, modified files)
                        display_pr_details(pr, owner, repo).await?;

                        // Display PR comments
                        display_pr_comments(pr_number, owner, repo)?;
                    } else {
                        println!("PR #{} not found in the current list.", pr_number);
                    }
                }
                Err(_) => println!("Invalid PR number."),
            }
        }
    }

    Ok(())
}
