docs:
	bundle exec asciidoctor -d book doc/*.adoc

clean:
	rm doc/*.html

.PHONY: docs clean
