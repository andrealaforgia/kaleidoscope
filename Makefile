# Kaleidoscope — one-command local experiment stack (ADR-0077).
#
# Thin wrapper over `docker compose` (compose.yaml). `make` is chosen
# over a justfile because it needs no extra tooling install (ADR-0077
# F2 / A5). No deploy target: Kaleidoscope deploys nothing; this is the
# local "one command, send, see" experience.
#
#   make up     bring the stack up, wait until healthy, print the URL
#   make down   stop the stack, PRESERVE the volume (durable telemetry)
#   make demo   push sample telemetry now (force, ignores the seed marker)
#   make seed   push sample telemetry once (marker-gated; no-op if seeded)
#   make logs   follow the runtime logs
#   make clean  stop the stack and REMOVE the volume (fresh empty stack)
#   make help   list targets
#
# Prefer these over raw compose. British English, no em-dashes.

COMPOSE ?= docker compose
PRISM_URL ?= http://localhost:9090

.DEFAULT_GOAL := help
.PHONY: help up down demo seed logs clean

help: ## List available targets
	@echo "Kaleidoscope local experiment stack — targets:"
	@grep -E '^[a-z][a-z-]*:.*## ' $(MAKEFILE_LIST) \
		| sed -E 's/^([a-z-]+):.*## (.*)/  make \1\t\2/' \
		| expand -t 14

up: ## Bring the stack up, wait until healthy, print the Prism URL
	@echo "==> Building and starting the consolidated runtime ..."
	@# `up -d --wait` reconciles to the desired state (a second `up` on a
	@# healthy stack is a no-op) and blocks until the runtime healthcheck
	@# passes. A required host port already in use surfaces here as a
	@# compose bind error and aborts with a non-zero exit (no half-up
	@# stack); a startup refusal fails the --wait and is surfaced too.
	$(COMPOSE) up -d --build --wait --wait-timeout 180 runtime
	@echo "==> Runtime healthy. Confirming the query/Prism origin answers ..."
	@# Host-side check of the actual HTTP origin (stronger than the
	@# in-container TCP healthcheck): Prism is served same-origin on 9090.
	@for i in $$(seq 1 30); do \
		if curl -fsS -o /dev/null "$(PRISM_URL)/"; then \
			echo "==> Prism is up:  $(PRISM_URL)"; \
			echo "    Query APIs:   :9090 metrics  :9091 logs  :9092 traces"; \
			echo "    OTLP ingest:  :4317 gRPC     :4318 HTTP"; \
			echo "    Next: 'make demo' to push sample telemetry, then open $(PRISM_URL)"; \
			exit 0; \
		fi; \
		sleep 2; \
	done; \
	echo "ERROR: runtime reported healthy but $(PRISM_URL) did not answer in time." >&2; \
	echo "       Inspect with 'make logs'." >&2; \
	exit 1

down: ## Stop the stack, preserving the named volume (durable telemetry)
	@echo "==> Stopping the stack (volume preserved) ..."
	$(COMPOSE) down

demo: up ## Push sample telemetry now (force; ignores the seed marker)
	@echo "==> Pushing sample telemetry (forced) ..."
	@# The stack is up (via the `up` prereq). Build + run the one-shot
	@# generator with SEED_FORCE=1 so it pushes regardless of the marker.
	@# Until DELIVER lands crates/kaleidoscope-telemetrygen the generator
	@# build fails fast with a clear cargo error (wired-ahead, ADR-0077 F3).
	$(COMPOSE) --profile seed run --rm --build -e SEED_FORCE=1 seed

seed: up ## Push sample telemetry once (marker-gated; no-op if already seeded)
	@echo "==> Seeding sample telemetry (once-only, marker-gated) ..."
	$(COMPOSE) --profile seed run --rm --build seed

logs: ## Follow the runtime logs
	$(COMPOSE) logs -f

clean: ## Stop the stack and REMOVE the volume (fresh empty stack)
	@echo "==> Stopping the stack and removing the volume ..."
	$(COMPOSE) down -v
