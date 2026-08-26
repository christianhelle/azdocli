// Theme toggle functionality
function toggleTheme() {
    const currentTheme = document.documentElement.getAttribute('data-theme');
    const newTheme = currentTheme === 'dark' ? 'light' : 'dark';

    document.documentElement.setAttribute('data-theme', newTheme);
    localStorage.setItem('theme', newTheme);

    // Update toggle button icon
    const toggleButton = document.querySelector('.theme-toggle');
    toggleButton.textContent = newTheme === 'dark' ? '☀️' : '🌙';
}

// Initialize theme on page load
function initTheme() {
    const savedTheme = localStorage.getItem('theme');
    const systemPrefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    const theme = savedTheme || (systemPrefersDark ? 'dark' : 'light');

    document.documentElement.setAttribute('data-theme', theme);

    // Update toggle button icon
    const toggleButton = document.querySelector('.theme-toggle');
    if (toggleButton) {
        toggleButton.textContent = theme === 'dark' ? '☀️' : '🌙';
    }
}

// Listen for system theme changes
window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (e) => {
    if (!localStorage.getItem('theme')) {
        const theme = e.matches ? 'dark' : 'light';
        document.documentElement.setAttribute('data-theme', theme);
        const toggleButton = document.querySelector('.theme-toggle');
        if (toggleButton) {
            toggleButton.textContent = theme === 'dark' ? '☀️' : '🌙';
        }
    }
});

// Mobile menu toggle
function toggleMobileMenu() {
    const navMenu = document.getElementById('nav-menu');
    const hamburger = document.getElementById('hamburger');
    const isOpen = navMenu.classList.toggle('active');
    hamburger.setAttribute('aria-expanded', String(isOpen));
}

// Initialize theme when DOM is ready
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initTheme);
} else {
    initTheme();
}
