FROM ruby:2.5


VOLUME /src

VOLUME /out

COPY Gemfile  Gemfile.lock /

RUN bundle install

WORKDIR /in

CMD for f in /in/src/bin/*.rs; do cp "$f" "/out/$(basename $f.txt)"; done && \
    bundle exec asciidoctor --doctype article --destination-dir /out /in/doc/*.adoc
