#!/usr/bin/env python3
"""
Validate snapcraft.yaml configuration without requiring snapcraft installation.

This script checks for common issues in snapcraft.yaml that would cause
'snapcraft pack' to fail.
"""

import yaml
import sys
from pathlib import Path


def validate_snapcraft_yaml(filepath='snapcraft.yaml'):
    """Validate snapcraft.yaml configuration."""
    
    print(f"Validating {filepath}...")
    print()
    
    try:
        with open(filepath, 'r') as f:
            config = yaml.safe_load(f)
    except FileNotFoundError:
        print(f"❌ Error: {filepath} not found")
        return False
    except yaml.YAMLError as e:
        print(f"❌ Error: Invalid YAML syntax: {e}")
        return False
    
    errors = []
    warnings = []
    
    # Check required fields
    required_fields = ['name', 'base', 'version', 'summary', 'description', 
                      'grade', 'confinement', 'parts', 'apps']
    
    for field in required_fields:
        if field not in config:
            errors.append(f"Missing required field: {field}")
    
    # Check architectures
    if 'architectures' not in config:
        errors.append("Missing 'architectures' field - required for multi-platform builds")
    else:
        print("✓ Architectures defined:")
        for arch in config['architectures']:
            if isinstance(arch, dict):
                if 'build-on' in arch:
                    print(f"  - {arch['build-on']}")
                else:
                    warnings.append(f"Architecture dict entry missing 'build-on': {arch}")
            elif isinstance(arch, str):
                print(f"  - {arch}")
            else:
                warnings.append(f"Invalid architecture entry format: {arch}")
    
    # Check confinement and plugs
    if config.get('confinement') == 'strict':
        print("✓ Using strict confinement")
        
        if 'apps' in config:
            for app_name, app_config in config['apps'].items():
                if 'plugs' not in app_config:
                    errors.append(
                        f"App '{app_name}' missing 'plugs' - required for strict confinement"
                    )
                else:
                    print(f"✓ App '{app_name}' has plugs:")
                    for plug in app_config.get('plugs', []):
                        print(f"  - {plug}")
    
    # Check parts
    if 'parts' in config:
        for part_name, part_config in config['parts'].items():
            if 'plugin' not in part_config:
                errors.append(f"Part '{part_name}' missing 'plugin' field")
            if 'source' not in part_config:
                errors.append(f"Part '{part_name}' missing 'source' field")
            
            # Check rust plugin specifics
            if part_config.get('plugin') == 'rust':
                print(f"✓ Part '{part_name}' uses rust plugin")
                if 'build-packages' in part_config:
                    print("  Build packages:")
                    for pkg in part_config['build-packages']:
                        print(f"    - {pkg}")
    
    print()
    
    # Report results
    if errors:
        print("❌ Validation FAILED:")
        print()
        for error in errors:
            print(f"  ❌ {error}")
        print()
    
    if warnings:
        print("⚠️  Warnings:")
        print()
        for warning in warnings:
            print(f"  ⚠️  {warning}")
        print()
    
    if not errors and not warnings:
        print("✅ Validation PASSED - snapcraft.yaml is properly configured!")
        print()
        return True
    elif not errors:
        print("✅ Validation PASSED with warnings")
        print()
        return True
    else:
        return False


if __name__ == '__main__':
    filepath = sys.argv[1] if len(sys.argv) > 1 else 'snapcraft.yaml'
    success = validate_snapcraft_yaml(filepath)
    sys.exit(0 if success else 1)
