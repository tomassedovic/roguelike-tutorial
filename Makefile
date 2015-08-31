all: clean docs

docs:
	@mkdir -p target/tutorial
	bundle exec asciidoctor --doctype article --destination-dir target/tutorial doc/*.adoc

clean:
	rm target/tutorial/*.html

.PHONY: all docs clean
