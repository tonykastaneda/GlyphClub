#import <Cocoa/Cocoa.h>

// The Glyph Club panel's contents. Plain Cocoa, no SDK: Panel.mm hosts it in
// Illustrator's docked panel and feeds it state.

typedef NS_ENUM(NSInteger, GCLogKind) {
	GCLogInfo,
	GCLogActivated,
	GCLogProblem,
};

@interface GCPanelView : NSView
@property (nonatomic, copy) void (^onAutoChanged)(BOOL on);
@property (nonatomic, copy) void (^onActivateNow)(void);
@property (nonatomic, copy) void (^onOpenGlyphClub)(void);

- (void)setConnected:(BOOL)connected text:(NSString*)text;
- (void)setAutoActivate:(BOOL)on;
- (void)setMissingCount:(NSInteger)count;
- (void)setBusy:(BOOL)busy;
- (void)log:(NSString*)line kind:(GCLogKind)kind;
- (void)setUpdateVersion:(NSString*)version;   // @"" hides the notice
@end
