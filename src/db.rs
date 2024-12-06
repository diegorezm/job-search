use rusqlite::Connection;
use std::{env, path::Path};

use crate::job::Job;

pub struct DB {
    conn: Connection,
}

impl DB {
    pub fn new() -> Self {
        let pool = DB::get_db();
        Self { conn: pool }
    }

    pub fn get_conn(&self) -> &Connection {
        &self.conn
    }

    pub fn close(self) {
        match self.conn.close() {
            Ok(_) => (),
            Err((conn, err)) => {
                eprintln!("Error closing database connection: {}", err);
                drop(conn);
            }
        }
    }

    fn get_db() -> Connection {
        // this will not work on windows.
        let home = env::var("HOME").expect("Could not get home directory.");
        let db_path = format!("{}/.local/share/job_search/job_search.db", home);

        let mut db_existed = true;

        if !Path::new(&db_path).exists() {
            db_existed = false;
            println!("Creating database at {}", db_path);
            std::fs::create_dir_all(Path::new(&db_path).parent().unwrap()).unwrap();
            std::fs::File::create(&db_path).unwrap();
        }

        let conn = Connection::open(&db_path).expect("Could not open database connection.");

        if !db_existed {
            DB::migrate_db(&conn);
        }
        conn
    }

    fn migrate_db(conn: &Connection) {
        let job_table = r#"
        CREATE TABLE jobs (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT NOT NULL,
            date DATE NOT NULL
        );
        "#;
        conn.execute(job_table, []).unwrap();
    }

    pub fn drop_db(&self) {
        let job_table = r#"DROP TABLE jobs;"#;
        self.conn
            .execute(job_table, [])
            .map_err(|e| println!("Error dropping database: {}", e))
            .unwrap();
        DB::migrate_db(&self.conn);
    }
}

pub struct Queries<'a> {
    conn: &'a Connection,
}

impl<'a> Queries<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn add_job(&self, title: String, description: String, date: String) {
        let _ = self
            .conn
            .execute(
                "INSERT INTO jobs (title, description, date) VALUES (?, ?, ?)",
                [title, description, date],
            )
            .expect("Error adding job");
    }

    pub fn list_jobs(&self) -> Vec<Job> {
        let mut rows = self
            .conn
            .prepare("SELECT id, title, description, date FROM jobs")
            .expect("Error preparing query for listing jobs");

        let mut jobs: Vec<Job> = Vec::new();

        let jobs_iter = rows
            .query_map([], |row| {
                Ok(Job {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    description: row.get(2)?,
                    date: row.get(3)?,
                })
            })
            .expect("Error listing jobs");

        for job in jobs_iter {
            match job {
                Ok(job) => jobs.push(job),
                Err(e) => eprintln!("Error processing job row: {}", e),
            }
        }

        jobs
    }

    pub fn seach_jobs(
        &self,
        title: Option<String>,
        description: Option<String>,
        date: Option<String>,
    ) -> Vec<Job> {
        let mut query = String::from("SELECT id, title, description, date FROM jobs");
        let mut args: Vec<String> = Vec::new();

        if let Some(title) = title {
            if !title.is_empty() {
                args.push(format!("title LIKE '%{}%'", title));
            }
        }

        if let Some(description) = description {
            if !description.is_empty() {
                args.push(format!("description LIKE '%{}%'", description));
            }
        }

        if let Some(date) = date {
            if !date.is_empty() {
                let formatted_date = match chrono::NaiveDate::parse_from_str(&date, "%d-%m-%Y") {
                    Ok(date) => date.format("%d-%m-%Y").to_string(),
                    Err(_) => "".to_string(),
                };
                if !formatted_date.is_empty() {
                    args.push(format!("date='{}'", formatted_date));
                } else {
                    eprintln!("Invalid date format. Please use the format dd-mm-yyyy.");
                }
            }
        }

        if args.len() > 0 {
            query.push_str(" WHERE ");
            query.push_str(&args.join(" AND "));
        }

        query.push_str(" ORDER BY date ASC");

        let mut rows = self
            .conn
            .prepare(&query)
            .map_err(|e| println!("Error preparing query for searching jobs: {}", e))
            .unwrap();

        let mut jobs: Vec<Job> = Vec::new();

        let jobs_iter = rows
            .query_map([], |row| {
                Ok(Job {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    description: row.get(2)?,
                    date: row.get(3)?,
                })
            })
            .expect("Error searching jobs");

        for job in jobs_iter {
            match job {
                Ok(job) => jobs.push(job),
                Err(e) => eprintln!("Error processing job row: {}", e),
            }
        }

        jobs
    }

    pub fn remove_job(&self, id: i32) {
        let _ = self
            .conn
            .execute("DELETE FROM jobs WHERE id = ?", [id])
            .expect("Error removing job");
    }
}
