all:
	@cp -r ../assets static/assets
	@awk -f ./riss.awk < ../README.md >content/_index.md
	@zola build
