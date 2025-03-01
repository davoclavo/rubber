use serde::Deserialize;
use serde_json::Value;
use std::env;
use std::error::Error;
use std::io::{self, BufRead, Write};

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

/// Analyzes a patch and returns insights about the changes
fn analyze_patch(patch: &str) -> (String, Vec<String>, Vec<String>) {
    let mut summary = String::new();
    let mut questions = Vec::new();
    let mut comments = Vec::new();

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

    (summary, questions, comments)
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

fn display_pr_details(pr: &PullRequest, owner: &str, repo: &str) -> Result<(), Box<dyn Error>> {
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
                        let (summary, questions, comments) = analyze_patch(&patch);

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

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <owner> <repo>", args[0]);
        std::process::exit(1);
    }

    let owner = &args[1];
    let repo = &args[2];

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
                        match display_pr_details(pr, owner, repo) {
                            Ok(_) => {}
                            Err(e) => println!("Error displaying PR details: {}", e),
                        }

                        // Display PR comments
                        match display_pr_comments(pr_number, owner, repo) {
                            Ok(_) => {}
                            Err(e) => println!("Error fetching comments: {}", e),
                        }
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
