#!/usr/bin/env perl
use strict;
use warnings;
use JSON;
use Getopt::Long;
use LWP::UserAgent;
use HTTP::Request;

# Conduit API Configuration
my $conduitUrl = $ENV{'CONDUIT_API_URL'} || 'http://127.0.0.1:4242';
my $conduitToken = $ENV{'CONDUIT_API_TOKEN'} || '';
my $cacheFile = $ENV{'CONDUIT_CACHE_FILE'} || '/tmp/conduit_queue_routes.cache.json';
my $logFile = $ENV{'CONDUIT_HOOK_LOG'} || '/tmp/conduit_copy2queue.log';

my $tID      = $ENV{'TR_TORRENT_ID'};
my $tHash    = $ENV{'TR_TORRENT_HASH'};
my $tName    = $ENV{'TR_TORRENT_NAME'};
my $tDir     = $ENV{'TR_TORRENT_DIR'};
my $tNode    = $ENV{'CONDUIT_NODE_NAME'} || $ENV{'TR_NODE_NAME'} || 'Transmission';
my $tTracker = $ENV{'TR_TORRENT_TRACKERS'} || $ENV{'TR_TORRENT_TRACKER'} || '';

GetOptions(
    'id|tid=i'   => \$tID,
    'hash=s'     => \$tHash,
    'name=s'     => \$tName,
    'dir=s'      => \$tDir,
    'node=s'     => \$tNode,
    'tracker=s'  => \$tTracker,
);

unless ($tName) {
    print "Usage: copy2queue.pl --id <id> --hash <hash> --name <name> --dir <dir> [--node <node>] [--tracker <tracker>]\n";
    exit 1;
}

my $tFullPath = $tDir ? "$tDir/$tName" : $tName;
writeLog("Processing torrent on [$tNode]: '$tName' (Hash: " . ($tHash || 'none') . ")");

# 1. Query Conduit API for 4-tier queue classification
my $classification = classifyFile($tName, $tHash, $tDir, $tNode, $tTracker);
my $targetDir   = $classification->{'target_dir'} || '/media/queue/miscQueue/';
my $postCmd     = $classification->{'post_cmd'} || 'cp -alv';
my $queueName   = $classification->{'queue'} || 'misc';
my $matchSource = $classification->{'match_source'} || 'default';
my $trace       = $classification->{'decision_trace'} || [];

writeLog("Classified '$tName' -> Queue '$queueName' ($targetDir) via $matchSource");
for my $step (@$trace) {
    writeLog("  -> $step");
}

# 2. Ensure target directory exists
use File::Path qw(make_path);
make_path($targetDir) unless -d $targetDir;

# 3. Execute copy/hardlink
my @cmd_parts = split(/\s+/, $postCmd || 'cp -alv');
my @full_cmd = (@cmd_parts, $tFullPath, $targetDir);
writeLog("Running: " . join(" ", map { qq("$_") } @full_cmd));
my $rc = system(@full_cmd);
if ($rc == 0) {
    writeLog("Successfully staged payload to $targetDir");
} else {
    writeLog("Warning: Staging command exited with code $rc");
}

# 4. Notify Conduit
notifyConduit($tHash, $tName, $tFullPath, $queueName, $targetDir, $tNode);

sub classifyFile {
    my ($name, $hash, $dir, $node, $tracker) = @_;
    my $ua = LWP::UserAgent->new(timeout => 5);
    my $req = HTTP::Request->new(POST => "$conduitUrl/api/sync/classify");
    $req->header('Content-Type' => 'application/json');
    if ($conduitToken) {
        $req->header('Authorization' => "Bearer $conduitToken");
    }
    my $body = encode_json({
        name    => $name,
        hash    => $hash || '',
        dir     => $dir || '',
        node    => $node || '',
        tracker => $tracker || '',
    });
    $req->content($body);

    my $res = eval { $ua->request($req) };
    if ($res && $res->is_success) {
        my $data = eval { decode_json($res->decoded_content) };
        if ($data) {
            # Cache locally
            if (open(my $fh, '>', $cacheFile)) {
                print $fh encode_json($data);
                close $fh;
            }
            return $data;
        }
    }

    # Fallback to local cache if API is offline
    if (-f $cacheFile && open(my $fh, '<', $cacheFile)) {
        local $/;
        my $cached = <$fh>;
        close $fh;
        my $data = eval { decode_json($cached) };
        return $data if $data;
    }

    # Offline heuristic fallback
    my $q = ($name =~ /S\d{2}E\d{2}|\d{1,2}x\d{1,2}/i) ? 'tv' : 'movie';
    $q .= 'UHD' if ($name =~ /2160p|UHD|4K/i);
    return {
        media_type   => $q,
        queue        => $q,
        target_dir   => "/media/queue/${q}Queue/",
        post_cmd     => "cp -alv",
        match_source => "offline_fallback",
    };
}

sub notifyConduit {
    my ($hash, $name, $path, $queue, $target, $node) = @_;
    my $ua = LWP::UserAgent->new(timeout => 5);
    my $req = HTTP::Request->new(POST => "$conduitUrl/api/sync/notify-download");
    $req->header('Content-Type' => 'application/json');
    if ($conduitToken) {
        $req->header('Authorization' => "Bearer $conduitToken");
    }
    my $body = encode_json({
        hash       => $hash || '',
        name       => $name,
        node       => $node || 'Transmission',
        path       => $path,
        queue      => $queue,
        target_dir => $target,
    });
    $req->content($body);
    eval { $ua->request($req) };
}

sub writeLog {
    my ($msg) = @_;
    print "[copy2queue.pl] $msg\n";
    if (open(my $fh, '>>', $logFile)) {
        print $fh "[copy2queue.pl] $msg\n";
        close $fh;
    }
}
