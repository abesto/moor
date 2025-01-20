// Populate the sidebar
//
// This is a script, and not included directly in the page, to control the total size of the book.
// The TOC contains an entry for each page, so if each page includes a copy of the TOC,
// the total size of the page becomes O(n**2).
class MDBookSidebarScrollbox extends HTMLElement {
    constructor() {
        super();
    }
    connectedCallback() {
        this.innerHTML = '<ol class="chapter"><li class="chapter-item expanded "><a href="foreword.html"><strong aria-hidden="true">1.</strong> Foreword</a></li><li class="chapter-item expanded "><a href="toaststunt-moo-resources.html"><strong aria-hidden="true">2.</strong> ToastStunt &amp; MOO Resources</a></li><li class="chapter-item expanded "><a href="introduction.html"><strong aria-hidden="true">3.</strong> Introduction</a></li><li class="chapter-item expanded "><a href="the-toaststunt-database.html"><strong aria-hidden="true">4.</strong> The ToastStunt Database</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="the-toaststunt-database/moo-value-types.html"><strong aria-hidden="true">4.1.</strong> MOO Value Types</a></li><li class="chapter-item expanded "><a href="the-toaststunt-database/objects-in-the-moo-database.html"><strong aria-hidden="true">4.2.</strong> Objects in the MOO Database</a></li></ol></li><li class="chapter-item expanded "><a href="the-built-in-command-parser.html"><strong aria-hidden="true">5.</strong> The Built-in Command Parser</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="the-built-in-command-parser/threading.html"><strong aria-hidden="true">5.1.</strong> Threading</a></li></ol></li><li class="chapter-item expanded "><a href="the-moo-programming-language.html"><strong aria-hidden="true">6.</strong> The MOO Programming Language</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="the-moo-programming-language/moo-language-expressions.html"><strong aria-hidden="true">6.1.</strong> MOO Language Expressions</a></li><li class="chapter-item expanded "><a href="the-moo-programming-language/moo-language-statements.html"><strong aria-hidden="true">6.2.</strong> MOO Language Statements</a></li><li class="chapter-item expanded "><a href="the-moo-programming-language/moo-tasks.html"><strong aria-hidden="true">6.3.</strong> MOO Tasks</a></li><li class="chapter-item expanded "><a href="the-moo-programming-language/built-in-functions.html"><strong aria-hidden="true">6.4.</strong> Built-in Functions</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="the-moo-programming-language/built-in-functions/object-oriented-programming.html"><strong aria-hidden="true">6.4.1.</strong> Object-Oriented Programming</a></li><li class="chapter-item expanded "><a href="the-moo-programming-language/built-in-functions/manipulating-moo-values.html"><strong aria-hidden="true">6.4.2.</strong> Manipulating MOO Values</a></li><li class="chapter-item expanded "><a href="the-moo-programming-language/built-in-functions/manipulating-objects.html"><strong aria-hidden="true">6.4.3.</strong> Manipulating Objects</a></li></ol></li><li class="chapter-item expanded "><a href="the-moo-programming-language/server-commands-and-database-assumptions.html"><strong aria-hidden="true">6.5.</strong> Server Commands and Database Assumptions</a></li><li class="chapter-item expanded "><a href="the-moo-programming-language/server-assumptions-about-the-database.html"><strong aria-hidden="true">6.6.</strong> Server Assumptions About the Database</a></li><li class="chapter-item expanded "><a href="the-moo-programming-language/networking.html"><strong aria-hidden="true">6.7.</strong> Networking</a></li><li class="chapter-item expanded "><a href="the-moo-programming-language/the-first-tasks-run-by-the-server.html"><strong aria-hidden="true">6.8.</strong> The First Tasks Run By the Server</a></li><li class="chapter-item expanded "><a href="the-moo-programming-language/controlling-the-execution-of-tasks.html"><strong aria-hidden="true">6.9.</strong> Controlling the Execution of Tasks</a></li><li class="chapter-item expanded "><a href="the-moo-programming-language/controlling-the-handling-of-aborted-tasks.html"><strong aria-hidden="true">6.10.</strong> Controlling the Handling of Aborted Tasks</a></li><li class="chapter-item expanded "><a href="the-moo-programming-language/matching-in-command-parsing.html"><strong aria-hidden="true">6.11.</strong> Matching in Command Parsing</a></li><li class="chapter-item expanded "><a href="the-moo-programming-language/restricting-access-to-built-in-properties-and-functions.html"><strong aria-hidden="true">6.12.</strong> Restricting Access to Built-in Properties and Functions</a></li><li class="chapter-item expanded "><a href="the-moo-programming-language/creating-and-recycling-objects.html"><strong aria-hidden="true">6.13.</strong> Creating and Recycling Objects</a></li><li class="chapter-item expanded "><a href="the-moo-programming-language/object-movement.html"><strong aria-hidden="true">6.14.</strong> Object Movement</a></li><li class="chapter-item expanded "><a href="the-moo-programming-language/temporarily-enabling-obsolete-server-features.html"><strong aria-hidden="true">6.15.</strong> Temporarily Enabling Obsolete Server Features</a></li><li class="chapter-item expanded "><a href="the-moo-programming-language/signals-to-the-server.html"><strong aria-hidden="true">6.16.</strong> Signals to the Server</a></li><li class="chapter-item expanded "><a href="the-moo-programming-language/server-configuration.html"><strong aria-hidden="true">6.17.</strong> Server Configuration</a></li></ol></li><li class="chapter-item expanded "><li class="spacer"></li><li class="chapter-item expanded "><a href="legal.html"><strong aria-hidden="true">7.</strong> Legal</a></li></ol>';
        // Set the current, active page, and reveal it if it's hidden
        let current_page = document.location.href.toString();
        if (current_page.endsWith("/")) {
            current_page += "index.html";
        }
        var links = Array.prototype.slice.call(this.querySelectorAll("a"));
        var l = links.length;
        for (var i = 0; i < l; ++i) {
            var link = links[i];
            var href = link.getAttribute("href");
            if (href && !href.startsWith("#") && !/^(?:[a-z+]+:)?\/\//.test(href)) {
                link.href = path_to_root + href;
            }
            // The "index" page is supposed to alias the first chapter in the book.
            if (link.href === current_page || (i === 0 && path_to_root === "" && current_page.endsWith("/index.html"))) {
                link.classList.add("active");
                var parent = link.parentElement;
                if (parent && parent.classList.contains("chapter-item")) {
                    parent.classList.add("expanded");
                }
                while (parent) {
                    if (parent.tagName === "LI" && parent.previousElementSibling) {
                        if (parent.previousElementSibling.classList.contains("chapter-item")) {
                            parent.previousElementSibling.classList.add("expanded");
                        }
                    }
                    parent = parent.parentElement;
                }
            }
        }
        // Track and set sidebar scroll position
        this.addEventListener('click', function(e) {
            if (e.target.tagName === 'A') {
                sessionStorage.setItem('sidebar-scroll', this.scrollTop);
            }
        }, { passive: true });
        var sidebarScrollTop = sessionStorage.getItem('sidebar-scroll');
        sessionStorage.removeItem('sidebar-scroll');
        if (sidebarScrollTop) {
            // preserve sidebar scroll position when navigating via links within sidebar
            this.scrollTop = sidebarScrollTop;
        } else {
            // scroll sidebar to current active section when navigating via "next/previous chapter" buttons
            var activeSection = document.querySelector('#sidebar .active');
            if (activeSection) {
                activeSection.scrollIntoView({ block: 'center' });
            }
        }
        // Toggle buttons
        var sidebarAnchorToggles = document.querySelectorAll('#sidebar a.toggle');
        function toggleSection(ev) {
            ev.currentTarget.parentElement.classList.toggle('expanded');
        }
        Array.from(sidebarAnchorToggles).forEach(function (el) {
            el.addEventListener('click', toggleSection);
        });
    }
}
window.customElements.define("mdbook-sidebar-scrollbox", MDBookSidebarScrollbox);
