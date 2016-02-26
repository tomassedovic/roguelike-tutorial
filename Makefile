all: clean docs

docs:
	@mkdir -p target/tutorial
	for f in src/bin/*.rs; do cp "$$f" "target/tutorial/$$(basename $$f.txt)"; done
	bundle exec asciidoctor --doctype article --destination-dir target/tutorial doc/*.adoc

preview: docs
	firefox --private-window file://$(shell readlink -f target/tutorial/index.html) 2>/dev/null

clean:
	rm -rf target/tutorial

.PHONY: all docs preview clean
