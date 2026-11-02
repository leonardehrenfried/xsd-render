NETEX      ?= ../NeTEx
NETEX_REF  ?= $(shell git -C $(NETEX) rev-parse --abbrev-ref HEAD)
NETEX_REPO ?= https://github.com/TransmodelEcosystem/NeTEx
OUT        ?= netex.html
BIN        := target/release/xsd-render

.PHONY: install netex $(BIN)

install:
	cargo build --release
	sudo cp target/release/xsd-render /usr/local/bin

$(BIN):
	cargo build --release

# Renders the NeTEx schema with links to its examples on GitHub
netex: $(BIN)
	$(BIN) $(NETEX)/xsd/NeTEx_publication.xsd -t "NeTEx XML Schema Viewer" -o $(OUT) \
		-e $(NETEX)/examples -u $(NETEX_REPO)/blob/$(NETEX_REF)/examples
	rsync --info=progress2 --perms --chmod=u+rwx,g+rwx,o+rwx netex.html leonard.io:www/transmodel/
