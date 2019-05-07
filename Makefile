all: clean docs

docs:
	@mkdir -p target/tutorial
	asciidoctor --destination-dir target/tutorial doc/*.adoc

publish:
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

list-contributor-names:
	git shortlog --summary | awk '{$$1=""}1' | sort

list-contributor-links:
	git log --merges | grep 'Merge pull request' | awk '{print $$6}' | cut -d/ -f1 | sort | uniq | sed -e 's|^|https://github.com/|'

.PHONY: all docs docs-docker preview clean publish list-contributor-names list-contributor-links
