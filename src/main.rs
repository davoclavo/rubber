use serde::Deserialize;
use serde_json::Value;
use std::env;
use std::error::Error;
use std::io::{self, BufRead, Write};

#[derive(Deserialize, Debug)]
struct PullRequest {
    number: u32,
    title: String,
    user: User,
    created_at: String,
    html_url: String,
    comments_url: String,
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

        for pr in response {
            // Truncate title if too long
            let title = if pr.title.len() > 47 {
                format!("{}...", &pr.title[..44])
            } else {
                pr.title
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

        println!("\nEnter PR number to view comments (or 'q' to quit): ");
        io::stdout().flush()?;

        let stdin = io::stdin();
        let mut input = String::new();
        stdin.lock().read_line(&mut input)?;

        let input = input.trim();
        if input.to_lowercase() != "q" {
            match input.parse::<u32>() {
                Ok(pr_number) => match display_pr_comments(pr_number, owner, repo) {
                    Ok(_) => {}
                    Err(e) => println!("Error fetching comments: {}", e),
                },
                Err(_) => println!("Invalid PR number."),
            }
        }
    }

    Ok(())
}
