docs:
	bundle exec asciidoctor -d article doc/*.adoc

clean:
	rm doc/*.html

.PHONY: docs clean
