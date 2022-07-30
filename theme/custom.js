// Open all external links in new tabs
Array.prototype.forEach.call(document.links, function(link) {
    if (link.hostname != window.location.hostname) {
        link.target = '_blank';
    }
})

