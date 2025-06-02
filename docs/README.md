# Documentation

This directory contains the GitHub Pages documentation for Azure DevOps CLI (azdocli).

## Files

- `index.html` - Main landing page with overview and quick start
- `user-guide.html` - Comprehensive user guide with all commands and features
- `contributing.html` - Contributing guidelines for developers
- `styles.css` - CSS styles for all documentation pages

## GitHub Pages

This documentation is automatically deployed to GitHub Pages using the workflow in `.github/workflows/static.yml`. The documentation is deployed from this `/docs` folder on every push to the main branch.

## Local Development

To preview the documentation locally:

1. Open any HTML file in your browser
2. Or use a local web server:
   ```bash
   # Using Python
   python -m http.server 8000
   
   # Using Node.js (if you have http-server installed)
   npx http-server
   ```

## Content

The documentation content is based on information from:
- `README.md` - Main project information
- `CONTRIBUTING.md` - Contributing guidelines
- Source code documentation and examples

## Updates

When updating the documentation:
1. Ensure all links work correctly
2. Update cross-references between pages
3. Test on different screen sizes (responsive design)
4. Validate HTML structure
5. Ensure content is up-to-date with the latest features