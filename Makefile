all: clean docs

docs:
	@mkdir -p target/tutorial
	for f in src/bin/*.rs; do cp "$$f" "target/tutorial/$$(basename $$f.txt)"; done
	bundle exec asciidoctor --doctype article --destination-dir target/tutorial doc/*.adoc

publish: docs
	@git diff-index --quiet HEAD || { echo "Error: the repository is dirty."; exit 1; }
	rm -rf .deploy
	cp -r target/tutorial .deploy
	git checkout gh-pages
	git pull
	rm -rf *
	mv .deploy/* .
	rm -d .deploy
	git add -A
	git commit -m "Updated"
	git push --force
	git checkout -

preview: docs
	firefox --private-window file://$(shell readlink -f target/tutorial/index.html) 2>/dev/null

clean:
	rm -rf target/tutorial

.PHONY: all docs preview clean publish
