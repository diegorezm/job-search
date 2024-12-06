#[macro_use]
extern crate prettytable;

mod db;
mod job;

use std::{fs::File, io::BufWriter, str::FromStr};

use chrono::{Local, NaiveDate};
use clap::{Args, Parser, Subcommand};
use db::DB;
use prettytable::{csv, Cell, Row, Table};
use std::io::Write;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct App {
    // List of commands
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Add a new job with title, description, and an optional date")]
    Add {
        title: String,
        description: String,
        date: Option<String>,
    },
    #[command(about = "List all jobs")]
    List,
    #[command(about = "Search for jobs, optionally filtering by title, description, and date")]
    Search(SearchArgs),
    #[command(about = "Remove a job by its id")]
    Remove { id: i32 },
    #[command(about = "Clear the database")]
    Clear,
    #[command(about = "Export jobs to a file.")]
    Export(ExportArgs),
}

#[derive(Args, Debug)]
struct ExportArgs {
    #[arg(long, default_value = "jobs.json", help = "File to export jobs to.")]
    file: String,
    #[arg(
        long,
        default_value = "json",
        help = "Format of the exported file. Options: json, csv"
    )]
    format: Format,
}

#[derive(Debug, Clone)]
enum Format {
    Json = 1,
    Csv = 2,
}

impl FromStr for Format {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "json" => Ok(Format::Json),
            "csv" => Ok(Format::Csv),
            _ => Err(format!("\nInvalid format: {}. Valid formats: json, csv", s)),
        }
    }
}

#[derive(Debug, Args)]
struct SearchArgs {
    #[arg(short, long, default_value = "")]
    title: Option<String>,
    #[arg(short, long, default_value = "")]
    description: Option<String>,
    #[arg(long, default_value = "")]
    date: Option<String>,
}

fn display_jobs(jobs: Vec<job::Job>) {
    let mut table = Table::new();
    table.set_format(*prettytable::format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
    table.set_titles(row!["ID", "Title", "Description", "Date"]);

    for job in jobs {
        table.add_row(Row::new(vec![
            Cell::new(&job.id.to_string()),
            Cell::new(&job.title),
            Cell::new(&job.description),
            Cell::new(&job.date),
        ]));
    }
    table.printstd();
}

fn format_date(date: String) -> String {
    if date == "today" {
        return Local::now().format("%d-%m-%Y").to_string();
    } else {
        let formatted_date = match NaiveDate::parse_from_str(&date, "%d-%m-%Y") {
            Ok(date) => date.format("%d-%m-%Y").to_string(),
            Err(_) => "".to_string(),
        };
        if formatted_date.is_empty() {
            panic!("Invalid date format. Please use the format dd-mm-yyyy.");
        }
        return formatted_date;
    }
}

fn export_jobs(jobs: Vec<job::Job>, format: Format, file: &str) {
    match format {
        Format::Json => {
            let f = File::create(file)
                .map_err(|e| eprintln!("Error opening file: {}", e))
                .expect("Error opening file");
            let mut writer = BufWriter::new(f);

            writeln!(writer, "[").expect("Error writing to file");

            for (i, job) in jobs.iter().enumerate() {
                let job_json = format!(
                    r#"{{"id": {}, "title": "{}", "description": "{}", "date": "{}"}}"#,
                    job.id, job.title, job.description, job.date
                );

                if i < jobs.len() - 1 {
                    writeln!(writer, "{},", job_json).expect("Error writing job to file");
                } else {
                    writeln!(writer, "{}", job_json).expect("Error writing job to file");
                }
            }

            writeln!(writer, "]").expect("Error closing JSON array");
        }
        Format::Csv => {
            let headers = vec!["id", "title", "description", "date"];
            let mut wtr = csv::Writer::from_path(file)
                .map_err(|e| eprintln!("Error opening file: {}", e))
                .unwrap();
            wtr.write_record(headers)
                .map_err(|e| eprintln!("Error writing headers: {}", e))
                .unwrap();
            for job in jobs {
                let row = vec![job.id.to_string(), job.title, job.description, job.date];
                wtr.write_record(row)
                    .map_err(|e| eprintln!("Error writing row: {}", e))
                    .unwrap();
            }
        }
    }
    println!("Jobs exported successfully to {}", file);
}

fn main() {
    let db = DB::new();
    let conn = db.get_conn();
    let queries = db::Queries::new(conn);
    let args = App::parse();
    match args.cmd {
        Commands::Add {
            title,
            description,
            date,
        } => {
            let d: String;

            if date.is_none() {
                d = format_date("today".to_string());
            } else {
                d = format_date(date.unwrap());
            }

            queries.add_job(title, description, d);
            println!("Job added successfully");
        }
        Commands::Search(args) => {
            let jobs = queries.seach_jobs(args.title, args.description, args.date);
            display_jobs(jobs);
        }
        Commands::List => {
            let jobs = queries.list_jobs();
            display_jobs(jobs);
        }
        Commands::Remove { id } => {
            queries.remove_job(id);
            println!("Job removed successfully");
        }
        Commands::Clear => {
            db.drop_db();
            println!("Database cleared successfully");
        }
        Commands::Export(args) => {
            let jobs = queries.list_jobs();
            export_jobs(jobs, args.format, args.file.as_str());
        }
    }

    db.close();
}
