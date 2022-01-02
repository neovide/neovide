.PHONY: md_update
md_update:
	@awk -f ./riss.awk < ../README.md >content/_index.md

.PHONY: content_update
content_update: md_update
	@mkdir -p static/assets
	@cp -r ../assets static/

.PHONY: all
all: content_update
	@zola build

.PHONY: serve
serve: content_update
	@zola serve
