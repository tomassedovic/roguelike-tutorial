all: clean docs

docs:
	@mkdir -p target/tutorial
	cp src/bin/*.rs target/tutorial
	bundle exec asciidoctor --doctype article --destination-dir target/tutorial doc/*.adoc

preview: docs
	firefox --private-window file://$(shell readlink -f target/tutorial/index.html) 2>/dev/null

clean:
	rm target/tutorial/*.html

.PHONY: all docs preview clean
