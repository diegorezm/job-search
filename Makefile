install:
	cargo build --release
	cp target/release/job_search ~/.local/bin/job_search

uninstall:
	rm ~/.local/bin/job_search

run:
	./target/debug/job_search
