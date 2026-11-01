# Morrow Local Release & Toolchain Automation
TAG ?= v2.3.0

.PHONY: release check build test bench package verify dry-run clean

# Full release pipeline: preflight -> build -> test -> bench -> package -> verify -> publish
release:
	python scripts/pipeline.py $(TAG)

# Full local verification and packaging without publishing
check:
	python scripts/pipeline.py $(TAG) --no-publish

# Compile self-hosting fixed-point bootstrap
build:
	python scripts/pipeline.py $(TAG) --from build --skip test --skip bench --skip package --skip verify --skip publish

# Run all test suites and build examples
test:
	python scripts/pipeline.py $(TAG) --from test --skip bench --skip package --skip verify --skip publish

# Run runtime benchmarks and C baselines
bench:
	python scripts/pipeline.py $(TAG) --from bench --skip package --skip verify --skip publish

# Create distribution zip and copy binaries into dist/
package:
	python scripts/pipeline.py $(TAG) --from package --skip publish

# Verify asset checksums and archive integrity
verify:
	python scripts/pipeline.py $(TAG) --from verify --skip publish

# Preview pipeline execution without modifying state
dry-run:
	python scripts/pipeline.py $(TAG) --dry-run

# Clean build and distribution artifacts
clean:
	python -c "import shutil; shutil.rmtree('target', ignore_errors=True); shutil.rmtree('dist/$(TAG)', ignore_errors=True)"
