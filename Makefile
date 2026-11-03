NETEX      ?= ../NeTEx
NETEX_REF  ?= $(shell git -C $(NETEX) rev-parse --abbrev-ref HEAD)
NETEX_REPO ?= https://github.com/TransmodelEcosystem/NeTEx
OUT        ?= netex.html
BIN        := target/release/xsd-render

BRANCHES   := v2.0 v2.1-wip
UPLOAD     := leonard.io:www/transmodel/

SIRI          ?= ../SIRI
SIRI_REMOTE   ?= upstream
SIRI_REPO     ?= https://github.com/SIRI-CEN/SIRI
SIRI_BRANCHES := v2.0 v2.1 v2.3 v2.4-wip
# Published name -> git branch, where they differ (there is no v2.0 branch)
SIRI_REF_v2.0 := 2.0q
siri_ref       = $(or $(SIRI_REF_$(1)),$(1))

.PHONY: install netex netex-branches $(addprefix netex-,$(BRANCHES)) \
	siri-branches $(addprefix siri-,$(SIRI_BRANCHES)) $(BIN)

install:
	cargo build --release
	sudo cp target/release/xsd-render /usr/local/bin

$(BIN):
	cargo build --release

# Renders the NeTEx schema with links to its examples on GitHub
netex: $(BIN)
	$(BIN) $(NETEX)/xsd/NeTEx_publication.xsd -t "NeTEx XML Schema Viewer" -o $(OUT) \
		-e $(NETEX)/examples -u $(NETEX_REPO)/blob/$(NETEX_REF)/examples
	rsync --info=progress2 --perms --chmod=D755,F644 $(OUT) $(UPLOAD)

# Renders every branch in BRANCHES to netex/<branch>/index.html on the server.
# The branches are exported from origin into build/ so the NeTEx checkout is
# left untouched
netex-branches: $(addprefix netex-,$(BRANCHES))

$(addprefix netex-,$(BRANCHES)): netex-%: $(BIN)
	git -C $(NETEX) fetch origin $*
	rm -rf build/netex-$*
	mkdir -p build/netex-$* build/www/netex/$*
	git -C $(NETEX) archive origin/$* xsd examples | tar -x -C build/netex-$*
	$(BIN) build/netex-$*/xsd/NeTEx_publication.xsd -t "NeTEx $* XML Schema" -o build/www/netex/$*/index.html \
		-e build/netex-$*/examples -u $(NETEX_REPO)/blob/$*/examples
	rsync --info=progress2 --perms --chmod=D755,F644 --relative build/www/./netex/$*/index.html $(UPLOAD)

# Renders every branch in SIRI_BRANCHES to siri/<branch>/index.html on the server,
# the same way as netex-branches
siri-branches: $(addprefix siri-,$(SIRI_BRANCHES))

$(addprefix siri-,$(SIRI_BRANCHES)): siri-%: $(BIN)
	git -C $(SIRI) fetch $(SIRI_REMOTE) $(call siri_ref,$*)
	rm -rf build/siri-$*
	mkdir -p build/siri-$* build/www/siri/$*
	git -C $(SIRI) archive $(SIRI_REMOTE)/$(call siri_ref,$*) xsd examples | tar -x -C build/siri-$*
	$(BIN) build/siri-$*/xsd/siri.xsd -t "SIRI $* XML Schema" -o build/www/siri/$*/index.html \
		-e build/siri-$*/examples -u $(SIRI_REPO)/blob/$(call siri_ref,$*)/examples
	rsync --info=progress2 --perms --chmod=D755,F644 --relative build/www/./siri/$*/index.html $(UPLOAD)
