#import "PanelView.h"

// Glyph Club's lime, the neon one on dark UI and the deeper olive on light
// (same pair as the app's theme.rs).
static NSColor* Accent(NSView* view)
{
	BOOL dark = [[view.effectiveAppearance bestMatchFromAppearancesWithNames:@[NSAppearanceNameAqua, NSAppearanceNameDarkAqua]]
		isEqualToString:NSAppearanceNameDarkAqua];
	return dark ? [NSColor colorWithSRGBRed:0x9a / 255.0 green:1.0 blue:0.0 alpha:1]
		: [NSColor colorWithSRGBRed:0x5c / 255.0 green:0x8a / 255.0 blue:0.0 alpha:1];
}

@interface GCDot : NSView
@property (nonatomic) BOOL on;
@end

@implementation GCDot
- (NSSize)intrinsicContentSize { return NSMakeSize(8, 8); }
- (void)setOn:(BOOL)on { _on = on; self.needsDisplay = YES; }
- (void)drawRect:(NSRect)dirty
{
	[(self.on ? Accent(self) : [NSColor tertiaryLabelColor]) setFill];
	[[NSBezierPath bezierPathWithOvalInRect:NSInsetRect(self.bounds, 0.5, 0.5)] fill];
}
@end

@implementation GCPanelView {
	GCDot* _dot;
	NSTextField* _status;
	NSSwitch* _autoSwitch;
	NSTextField* _missing;
	NSButton* _activateButton;
	NSProgressIndicator* _spinner;
	NSTextView* _feed;
	NSStackView* _updateRow;
	NSTextField* _updateText;
	NSDateFormatter* _time;
	NSInteger _missingCount;
	BOOL _busy;
}

static NSTextField* Label(NSString* text, CGFloat size, NSFontWeight weight, NSColor* color)
{
	NSTextField* label = [NSTextField labelWithString:text];
	label.font = [NSFont systemFontOfSize:size weight:weight];
	label.textColor = color;
	label.lineBreakMode = NSLineBreakByTruncatingTail;
	[label setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
	return label;
}

- (instancetype)initWithFrame:(NSRect)frame
{
	if (!(self = [super initWithFrame:frame])) return nil;
	self.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
	_time = [NSDateFormatter new];
	_time.dateStyle = NSDateFormatterNoStyle;
	_time.timeStyle = NSDateFormatterShortStyle;

	NSTextField* title = Label(@"Glyph Club", 13, NSFontWeightSemibold, [NSColor labelColor]);
	_dot = [GCDot new];
	_status = Label(@"Looking for Glyph Club…", 11, NSFontWeightRegular, [NSColor secondaryLabelColor]);
	NSStackView* statusRow = [NSStackView stackViewWithViews:@[_dot, _status]];
	statusRow.spacing = 6;

	NSTextField* autoLabel = Label(@"Auto-activate missing fonts", 11, NSFontWeightRegular, [NSColor labelColor]);
	_autoSwitch = [NSSwitch new];
	_autoSwitch.controlSize = NSControlSizeSmall;
	_autoSwitch.target = self;
	_autoSwitch.action = @selector(autoToggled:);
	NSView* spacer = [NSView new];
	[spacer setContentHuggingPriority:1 forOrientation:NSLayoutConstraintOrientationHorizontal];
	NSStackView* autoRow = [NSStackView stackViewWithViews:@[autoLabel, spacer, _autoSwitch]];

	_missing = Label(@"", 11, NSFontWeightRegular, [NSColor secondaryLabelColor]);
	_activateButton = [NSButton buttonWithTitle:@"Activate Missing Fonts" target:self action:@selector(activateClicked:)];
	_activateButton.controlSize = NSControlSizeSmall;
	_activateButton.font = [NSFont systemFontOfSize:11];
	_spinner = [NSProgressIndicator new];
	_spinner.style = NSProgressIndicatorStyleSpinning;
	_spinner.controlSize = NSControlSizeSmall;
	_spinner.displayedWhenStopped = NO;
	NSStackView* actionRow = [NSStackView stackViewWithViews:@[_activateButton, _spinner]];
	actionRow.spacing = 6;

	// Shown only when GlyphClub ships a newer plug-in than this one: only
	// GlyphClub can install it (Settings > Integrations).
	_updateText = Label(@"", 11, NSFontWeightMedium, [NSColor systemOrangeColor]);
	NSTextField* updateHint = Label(@"Update it from Glyph Club's Settings.", 10, NSFontWeightRegular, [NSColor secondaryLabelColor]);
	NSButton* openButton = [NSButton buttonWithTitle:@"Open Glyph Club" target:self action:@selector(openClicked:)];
	openButton.controlSize = NSControlSizeSmall;
	openButton.font = [NSFont systemFontOfSize:11];
	_updateRow = [NSStackView stackViewWithViews:@[_updateText, updateHint, openButton]];
	_updateRow.orientation = NSUserInterfaceLayoutOrientationVertical;
	_updateRow.alignment = NSLayoutAttributeLeading;
	_updateRow.spacing = 4;
	_updateRow.hidden = YES;

	NSTextField* recent = Label(@"RECENT", 9, NSFontWeightSemibold, [NSColor tertiaryLabelColor]);

	NSScrollView* scroll = [NSScrollView new];
	scroll.hasVerticalScroller = YES;
	scroll.drawsBackground = NO;
	scroll.borderType = NSNoBorder;
	_feed = [NSTextView new];
	_feed.editable = NO;
	_feed.selectable = YES;
	_feed.drawsBackground = NO;
	_feed.textContainerInset = NSMakeSize(0, 2);
	_feed.textContainer.widthTracksTextView = YES;
	_feed.autoresizingMask = NSViewWidthSizable;
	scroll.documentView = _feed;
	[scroll setContentHuggingPriority:1 forOrientation:NSLayoutConstraintOrientationVertical];

	NSStackView* stack = [NSStackView stackViewWithViews:@[title, statusRow, _updateRow, autoRow, _missing, actionRow, recent, scroll]];
	stack.detachesHiddenViews = YES;
	stack.orientation = NSUserInterfaceLayoutOrientationVertical;
	stack.alignment = NSLayoutAttributeLeading;
	stack.spacing = 8;
	stack.edgeInsets = NSEdgeInsetsMake(12, 12, 12, 12);
	[stack setCustomSpacing:2 afterView:title];
	[stack setCustomSpacing:14 afterView:statusRow];
	[stack setCustomSpacing:14 afterView:_updateRow];
	[stack setCustomSpacing:4 afterView:autoRow];
	[stack setCustomSpacing:16 afterView:actionRow];
	[stack setCustomSpacing:2 afterView:recent];
	stack.translatesAutoresizingMaskIntoConstraints = NO;
	[self addSubview:stack];
	[NSLayoutConstraint activateConstraints:@[
		[stack.leadingAnchor constraintEqualToAnchor:self.leadingAnchor],
		[stack.trailingAnchor constraintEqualToAnchor:self.trailingAnchor],
		[stack.topAnchor constraintEqualToAnchor:self.topAnchor],
		[stack.bottomAnchor constraintEqualToAnchor:self.bottomAnchor],
		[autoRow.widthAnchor constraintEqualToAnchor:stack.widthAnchor constant:-24],
		[statusRow.widthAnchor constraintLessThanOrEqualToAnchor:stack.widthAnchor constant:-24],
		[scroll.widthAnchor constraintEqualToAnchor:stack.widthAnchor constant:-24],
		[scroll.heightAnchor constraintGreaterThanOrEqualToConstant:60],
	]];
	[self setMissingCount:0];
	return self;
}

- (BOOL)isFlipped { return YES; }

- (void)autoToggled:(NSSwitch*)sender
{
	if (self.onAutoChanged) self.onAutoChanged(sender.state == NSControlStateValueOn);
}

- (void)openClicked:(id)sender
{
	if (self.onOpenGlyphClub) self.onOpenGlyphClub();
}

- (void)setUpdateVersion:(NSString*)version
{
	_updateRow.hidden = version.length == 0;
	_updateText.stringValue = [NSString stringWithFormat:@"Plug-in update %@ available", version];
}

- (void)activateClicked:(id)sender
{
	if (self.onActivateNow) self.onActivateNow();
}

- (void)setConnected:(BOOL)connected text:(NSString*)text
{
	_dot.on = connected;
	_status.stringValue = text;
}

- (void)setAutoActivate:(BOOL)on
{
	_autoSwitch.state = on ? NSControlStateValueOn : NSControlStateValueOff;
}

- (void)refreshAction
{
	_activateButton.enabled = !_busy && _missingCount > 0;
	if (_busy) [_spinner startAnimation:nil];
	else [_spinner stopAnimation:nil];
}

- (void)setMissingCount:(NSInteger)count
{
	_missingCount = count;
	_missing.stringValue = count == 0 ? @"No missing fonts in open documents"
		: count == 1 ? @"1 missing font in open documents"
		: [NSString stringWithFormat:@"%ld missing fonts in open documents", (long) count];
	_missing.textColor = count ? [NSColor labelColor] : [NSColor secondaryLabelColor];
	[self refreshAction];
}

- (void)setBusy:(BOOL)busy
{
	_busy = busy;
	[self refreshAction];
}

- (void)log:(NSString*)line kind:(GCLogKind)kind
{
	NSColor* mark = kind == GCLogActivated ? Accent(self)
		: kind == GCLogProblem ? [NSColor systemOrangeColor] : [NSColor tertiaryLabelColor];
	NSMutableAttributedString* entry = [NSMutableAttributedString new];
	[entry appendAttributedString:[[NSAttributedString alloc] initWithString:@"● "
		attributes:@{NSForegroundColorAttributeName: mark, NSFontAttributeName: [NSFont systemFontOfSize:8]}]];
	[entry appendAttributedString:[[NSAttributedString alloc] initWithString:line
		attributes:@{NSForegroundColorAttributeName: [NSColor labelColor], NSFontAttributeName: [NSFont systemFontOfSize:11]}]];
	[entry appendAttributedString:[[NSAttributedString alloc] initWithString:[NSString stringWithFormat:@"  %@\n", [_time stringFromDate:[NSDate date]]]
		attributes:@{NSForegroundColorAttributeName: [NSColor tertiaryLabelColor], NSFontAttributeName: [NSFont systemFontOfSize:10]}]];

	// Newest first, and only the last 50.
	NSTextStorage* storage = _feed.textStorage;
	[storage insertAttributedString:entry atIndex:0];
	NSString* text = storage.string;
	NSUInteger lines = 0, cut = NSNotFound;
	for (NSUInteger i = 0; i < text.length; i++) {
		if ([text characterAtIndex:i] == '\n' && ++lines == 50) { cut = i + 1; break; }
	}
	if (cut != NSNotFound && cut < text.length) [storage deleteCharactersInRange:NSMakeRange(cut, text.length - cut)];
}

@end
