# Morrow Local Release & Toolchain Automation
# Stages: preflight -> version -> build -> test -> bench -> package -> verify -> publish
TAG ?= v2.4.0

.PHONY: release check version build test bench package verify dry-run clean

# Full release pipeline (bumps versions, rebuilds, verifies, publishes)
release:
	python scripts/pipeline.py $(TAG)

# Full local verification and packaging without publishing
check:
	python scripts/pipeline.py $(TAG) --no-publish

# Bump every version declaration to $(TAG) without building
version:
	python scripts/pipeline.py $(TAG) --from version --skip build --skip test --skip bench --skip package --skip verify --skip publish

# Compile the self-hosting fixed point (requires declarations to already match $(TAG))
build:
	python scripts/pipeline.py $(TAG) --from build --skip test --skip bench --skip package --skip verify --skip publish

# Run all test suites and build examples
test:
	python scripts/pipeline.py $(TAG) --from test --skip bench --skip package --skip verify --skip publish

# Run runtime benchmarks and C baselines
bench:
	python scripts/pipeline.py $(TAG) --from bench --skip package --skip verify --skip publish

# Create distribution zip, assets and release notes in dist/$(TAG)/
package:
	python scripts/pipeline.py $(TAG) --from package --skip verify --skip publish

# Verify asset checksums and archive integrity
verify:
	python scripts/pipeline.py $(TAG) --from verify --skip publish

# Preview pipeline execution without modifying state
dry-run:
	python scripts/pipeline.py $(TAG) --dry-run

# Clean build and distribution artifacts
clean:
	python -c "import shutil; shutil.rmtree('target', ignore_errors=True); shutil.rmtree('dist/$(TAG)', ignore_errors=True)"
